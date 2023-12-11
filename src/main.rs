use anyhow::Result;
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

// struct Boundrs {
//     label: Labeling,
//     relabel: Relabeling,
//     conflicts: Option<Conflicts>,
//     mode: Mode,
//     tagging: TaggingTool,
// }

// impl Boundrs {
//     fn handle_mode_switch(&mut self, ctx: &Context) {
//         // We save the current label and update the state in the new mode
//         if ctx.input(|i| i.key_pressed(Key::Tab)) {
//             // TODO this desperately needs to be refactored. Maybe label and relabel as tools (not owning their datatsets), instead of apps
//             self.mode = match self.mode {
//                 Mode::Label => {
//                     let sync = self.label.prepare_switch();
//                     self.relabel.refresh_after_switch(&sync, ctx);
//                     Mode::Relabel
//                 }
//                 Mode::Relabel => {
//                     // let (_, current_pos, _) = self.relabel.old_dataset.get_progress();
//                     // TODO fix this
//                     let sync = self.relabel.prepare_switch();
//                     if self.conflicts.is_none() {
//                         self.label.refresh_after_switch(&sync, ctx);
//                         Mode::Label
//                     } else {
//                         self.conflicts
//                             .as_mut()
//                             .unwrap()
//                             .refresh_after_switch(&sync, ctx);
//                         Mode::Conflicts
//                     }
//                 }
//                 Mode::Conflicts => {
//                     let sync = self.conflicts.as_mut().unwrap().prepare_switch();
//                     self.label.refresh_after_switch(&sync, ctx);
//                     Mode::Label
//                 }
//             };
//         }
//     }

//     fn new(cc: &CreationContext, args: Args) -> Self {
//         let label_config = DynLabelConfig::load_from_file(&args.config)
//             .expect("./labels.toml should exists as described in github repo");
//         let label_state = Labeling::new(&cc.egui_ctx, &args.data_dir, &args.prefix, label_config);
//         let label_config = DynLabelConfig::load_from_file(&args.config)
//             .expect("./labels.toml should exists as described in github repo");
//         let relabel_config = DynLabelConfig::load_from_file(&args.config_relabel)
//             .expect("./labels.toml should exists as described in github repo");
//         let relabel_state = Relabeling::new(
//             &cc.egui_ctx,
//             &args.data_dir,
//             &args.prefix,
//             &args.prefix_relabel,
//             label_config,
//             relabel_config,
//         );

//         let conflicts = args
//             .conflicts_dir
//             .map(|dir| Conflicts::from_dir(&cc.egui_ctx, dir).unwrap());

//         Self {
//             label: label_state,
//             relabel: relabel_state,
//             conflicts,
//             mode: Mode::Label,
//             tagging: TaggingTool::default(),
//         }
//     }
// }

// impl eframe::App for Boundrs {
//     fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
//         egui::Window::new("Boundrs").show(ctx, |ui| {
//             // ui.horizontal(|ui| {
//             //     ui.selectable_value(&mut self.mode, Mode::Label, "Label");
//             //     ui.selectable_value(&mut self.mode, Mode::Relabel, "Relabel");
//             // });
//             ui.label(format!("Current mode: {:?}", self.mode));
//             ui.separator();

//             match self.mode {
//                 Mode::Label => self.label.draw_ui(ui, ctx),
//                 Mode::Relabel => self.relabel.draw_ui(ui, ctx),
//                 Mode::Conflicts => self.conflicts.as_mut().unwrap().draw_ui(ui, ctx),
//             }
//         });
//         self.tagging.draw_ui(ctx, self.label.dataset.current_name());
//         egui::CentralPanel::default()
//             .frame(egui::Frame::none().fill(Color32::BLACK))
//             .show(ctx, |ui| {
//                 // Draw image

//                 self.handle_mode_switch(ctx);

//                 let gain_focus = !self.tagging.is_open;
//                 match self.mode {
//                     Mode::Label => self.label.draw_central_panel(ctx, ui, gain_focus),
//                     Mode::Relabel => self.relabel.draw_central_panel(ctx, ui, gain_focus),
//                     Mode::Conflicts => self.conflicts.as_mut().unwrap().draw_central_panel(ctx, ui),
//                 }
//             });
//     }
// }

struct Tools {
    label: Labeling,
    relabel: Relabeling,
    mode: ActiveTool,
}

impl Tools {
    fn active_mut(&mut self) -> &mut dyn Tool {
        match self.mode {
            ActiveTool::Label => &mut self.label,
            ActiveTool::Relabel => todo!(),
            ActiveTool::Conflicts => todo!(),
            ActiveTool::Tagging => todo!(),
        }
    }
    fn active(&self) -> &dyn Tool {
        match self.mode {
            ActiveTool::Label => &self.label,
            ActiveTool::Relabel => todo!(),
            ActiveTool::Conflicts => todo!(),
            ActiveTool::Tagging => todo!(),
        }
    }
    fn cylce(&mut self) {
        use ActiveTool as T;
        let modes = [T::Label];
        let current_index = modes.iter().position(|m| self.mode == *m).unwrap_or(0);
        let next_index = (current_index + 1) % modes.len();
        self.mode = modes[next_index];
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum ActiveTool {
    Label,
    Relabel,
    Conflicts,
    Tagging,
}
struct BoundrsV2 {
    // current_tool: Box<dyn Tool>,
    // tagging: TaggingTool,
    tools: Tools,
    dataset: Dataset,
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
        let label_config = DynLabelConfig::load_from_file(&args.config).unwrap();
        let label = Labeling::new(
            ctx,
            &args.data_dir,
            &args.prefix,
            label_config.clone(),
            dataset.current(),
        );
        let relabel_config = DynLabelConfig::load_from_file(&args.config_relabel).unwrap();
        let relabel = Relabeling::new(
            &args.data_dir,
            &args.prefix,
            &args.prefix_relabel,
            label_config,
            relabel_config,
        );

        // let conflicts = args
        //     .conflicts_dir
        //     .map(|dir| Conflicts::from_dir(&cc.egui_ctx, dir).unwrap());

        BoundrsV2 {
            dataset,
            tools: Tools {
                label,
                relabel,
                mode: ActiveTool::Label,
            }, // conflicts,
        }
    }
}

impl BoundrsV2 {
    fn go(&mut self, movement: DatasetMovement) -> Result<()> {
        self.tools.active().save_state(self.dataset.current())?;
        self.dataset.go(movement, None)?;
        self.tools
            .active_mut()
            .refresh_state(self.dataset.current())?;
        Ok(())
    }

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
            self.tools.cylce();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
            self.go(DatasetMovement::Previous).unwrap();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
            self.go(DatasetMovement::Next).unwrap();
        }

        // draw own ui
        egui::Window::new("Boundrs").show(ctx, |ui| {
            self.draw_top_ui(ui);
            ui.separator();
            // draw tool ui
            self.tools.active_mut().draw_ui(ui).unwrap();
        });

        // draw central panel with image
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let response = ui.image(self.dataset.current_img_uri());
                self.tools
                    .active_mut()
                    .draw_in_central_panel(ui, response, &self.dataset)
            });
    }
}
