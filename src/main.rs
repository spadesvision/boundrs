use anyhow::Result;
use boundrs::conflicts::Conflicts;
use boundrs::file_loader::BlockingFileLoader;
use boundrs::Tool;
use eframe::{egui, CreationContext};
use egui::*;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::path::PathBuf;

use boundrs::dataset::{Dataset, DatasetMovement, DynLabelConfig};

// use boundrs::conflicts::Conflicts;
use boundrs::labeling::Labeling;
use boundrs::relabeling::Relabeling;

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    data_dir: PathBuf,

    #[arg(long, default_value = "")]
    prefix: String,

    #[arg(long, default_value = "new_")]
    prefix_relabel: String,

    #[arg(long, default_value = "./labels_13.toml")]
    config: PathBuf,

    #[arg(long, default_value = "./labels_52.toml")]
    config_relabel: PathBuf,

    #[arg(short, long)]
    conflicts_dir: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 220.0]),
        ..Default::default()
    };
    eframe::run_native(
        "eframe template",
        native_options,
        Box::new(|cc| Box::new(BoundrsV2::new(cc, args))),
    )
    .unwrap();
}

#[derive(Debug, Deserialize, Serialize)]
struct TagEntry {
    image: String,
    tag: String,
}

#[derive(Default)]
struct TaggingTool {
    is_open: bool,
    data: TaggingToolData,
}

#[derive(Default)]
struct TaggingToolData {
    input: String,
    current_tags: Vec<String>,
}

impl TaggingToolData {
    fn draw_tags(&mut self, ui: &mut Ui) {
        ui.label("Current Tags:");
        ui.horizontal_wrapped(|ui| {
            let mut to_remove = None;
            for (index, tag) in self.current_tags.iter().enumerate() {
                if ui.button(tag).clicked() {
                    to_remove = Some(index);
                }
            }
            if let Some(index) = to_remove {
                self.current_tags.remove(index);
            }
        });
    }
    fn load_tags(&mut self, current_label_jpg: &str) -> Result<(), csv::Error> {
        if !PathBuf::from("tags.csv").exists() {
            let mut wtr = csv::Writer::from_path("tags.csv")?;
            wtr.write_record(["image", "tag"])?;
            wtr.flush()?;
        }
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path("tags.csv")?;
        self.current_tags.clear();
        let mut other_tags = vec![];
        for result in rdr.deserialize() {
            let record: TagEntry = result?;
            if record.image == current_label_jpg {
                self.current_tags.push(record.tag);
            } else {
                other_tags.push(record);
            }
        }
        // Overwriting tags.csv with the other_tags
        // println!("Saving {:?} tags", other_tags);
        println!("Saving {} tags", other_tags.len());
        let mut wtr = csv::Writer::from_path("tags.csv")?;
        if other_tags.is_empty() {
            wtr.write_record(["image", "tag"])?;
        }
        for tag_entry in other_tags {
            wtr.serialize(tag_entry)?;
        }
        wtr.flush()?;
        Ok(())
    }
    fn save_tags(&self, current_label_jpg: &str) -> Result<(), csv::Error> {
        let file = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true) // Creates the file if it does not exist
            .open("tags.csv")?;

        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);
        for tag in &self.current_tags {
            let entry = TagEntry {
                image: current_label_jpg.to_string(),
                tag: tag.to_string(),
            };
            wtr.serialize(entry)?;
        }
        println!("Saving additional {} tags", self.current_tags.len());
        wtr.flush()?;
        Ok(())
    }
}

impl TaggingTool {
    fn handle_keys(&mut self, ctx: &Context, current_jpg: &str) -> Result<()> {
        if !self.is_open && ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.data.load_tags(current_jpg)?;
            self.is_open = true;
        }
        if self.is_open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.data.save_tags(current_jpg)?;
            self.is_open = false;
        }
        Ok(())
    }

    fn draw_ui(&mut self, ctx: &Context, current_jpg: &str) {
        self.handle_keys(ctx, current_jpg).unwrap();
        if self.is_open {
            egui::Window::new("Tagging Tool")
                .open(&mut self.is_open)
                .show(ctx, |ui| {
                    // show the current tags
                    self.data.draw_tags(ui);

                    let response = ui.text_edit_singleline(&mut self.data.input);
                    response.request_focus();
                    if response.gained_focus() {
                        println!("gained focus");
                    }

                    // Add other UI elements for tagging as needed
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        println!("Adding tag {}", self.data.input);
                        self.data.current_tags.push(self.data.input.take());
                    }
                    // response.request_focus();
                });
        }
    }
}

struct Tools {
    all_tools: Vec<Box<dyn Tool>>,
    current_tool: usize,
}

impl Tools {
    fn active_mut(&mut self) -> &mut dyn Tool {
        return self.all_tools[self.current_tool].as_mut();
    }
    fn active(&self) -> &dyn Tool {
        return self.all_tools[self.current_tool].as_ref();
    }
    fn cylce(&mut self) {
        self.current_tool = (self.current_tool + 1) % self.all_tools.len()
    }

    fn init(args: Args, dataset: &Dataset) -> Tools {
        let mut all_tools: Vec<Box<dyn Tool>> = vec![];
        let label_config = DynLabelConfig::load_from_file(&args.config).unwrap();
        let label = Labeling::new(label_config.clone(), dataset.current());
        all_tools.push(Box::new(label));

        let relabel_config = DynLabelConfig::load_from_file(&args.config_relabel).unwrap();
        let relabel = Relabeling::new(
            &args.prefix,
            &args.prefix_relabel,
            label_config,
            relabel_config,
            dataset.current(),
        );
        all_tools.push(Box::new(relabel));

        if let Some(dir) = args.conflicts_dir {
            let conflicts = Conflicts::from_dir(dir, dataset.current()).unwrap();
            all_tools.push(Box::new(conflicts));
        }

        Tools {
            all_tools,
            current_tool: 0,
        }
    }
}

struct BoundrsV2 {
    // current_tool: Box<dyn Tool>,
    // tagging: TaggingTool,
    tools: Tools,
    dataset: Dataset,
    img_needs_focus: bool,
    // conflicts: Conflicts,
}

impl BoundrsV2 {
    fn new(cc: &CreationContext, args: Args) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_visuals(egui::Visuals {
            image_loading_spinners: false,
            ..Default::default()
        });
        let dataset = Dataset::from_input_dir(&args.data_dir).unwrap();
        egui_extras::install_image_loaders(ctx);
        ctx.add_bytes_loader(std::sync::Arc::new(BlockingFileLoader::default()));

        // let label = Labeling::new();
        let tools = Tools::init(args, &dataset);
        BoundrsV2 {
            dataset,
            tools,
            img_needs_focus: true,
        }
    }
}

impl BoundrsV2 {
    fn draw_top_ui(&mut self, ui: &mut Ui) {
        ui.label(format!("Active Tool: {:?}", self.tools.active().name()));
        let filename = self.dataset.current_name();
        ui.horizontal(|ui| {
            ui.label("Current image:");
            ui.label(filename);
        });
        ui.horizontal(|ui| {
            ui.label("Progress");
            ui.add(DragValue::from_get_set(|new_pos| {
                if let Some(new_pos) = new_pos {
                    self.dataset
                        .go(DatasetMovement::JumpTo(new_pos as usize), None)
                        .unwrap();
                    self.tools
                        .active_mut()
                        .refresh_state(self.dataset.current())
                        .unwrap();
                }
                self.dataset.get_progress().1 as f64
            }));
            let (_, current, max) = self.dataset.get_progress();
            ui.add(
                ProgressBar::new(current as f32 / max as f32)
                    .show_percentage()
                    .text(format!("{current} out of {max} images")),
            );
        });
    }
}

impl eframe::App for BoundrsV2 {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Handle global shortcuts
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Space)) {
            let current = self.dataset.current();
            self.tools.active().save_state(current).unwrap();
            self.tools.cylce();
            self.tools.active_mut().refresh_state(current).unwrap();
            self.img_needs_focus = true;
        }

        // draw own ui
        let res = egui::Window::new("Boundrs").show(ctx, |ui| {
            self.draw_top_ui(ui);
            ui.separator();
            // draw tool ui
            self.tools.active_mut().draw_ui(ui).unwrap();
        });
        if let Some(inner) = res {
            if inner.response.clicked() {
                inner.response.request_focus()
            }
            if inner.response.clicked_elsewhere() || inner.response.lost_focus() {
                self.img_needs_focus = true
            }
        }

        // draw central panel with image
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let img =
                    egui::Image::new(self.dataset.current_img_uri()).sense(Sense::click_and_drag());
                let response = ui.add(img);

                if self.img_needs_focus {
                    self.img_needs_focus = false;
                    response.request_focus();
                }

                self.tools
                    .active_mut()
                    .draw_in_central_panel(ui, response, &mut self.dataset)
            });
    }
}
