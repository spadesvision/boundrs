use sv_detections::{BBoxCardPose, YoloBBox};
use boundrs::file_loader::{BlockingFileLoader, ImageCrateLoader};
use boundrs::shady::ShadyFinder;
use boundrs::Tool;
use eframe::{egui, CreationContext};
use egui::*;
use std::path::PathBuf;

use boundrs::dataset::{BoundrsDataset, BoundrsDetection, DatasetMovement, DynLabelConfig};

use boundrs::check::check_differences;
use boundrs::conflicts::Conflicts;
use boundrs::labeling::Labeling;
use boundrs::relabeling::Relabeling;
use boundrs::tagging::Tagging;

use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Diffs(DiffsArgs),
    FindShady(FindShadyArgs),
    Label(LabelArgs),
}

#[derive(Parser, Debug)]
struct FindShadyArgs {
    data_dir: PathBuf,
}

#[derive(Parser, Debug)]
struct DiffsArgs {
    #[arg(long, default_value = "")]
    prefix: String,

    #[arg(long, default_value = "new_")]
    prefix_relabel: String,

    #[arg(long, default_value = "./labels_13.toml")]
    config: PathBuf,

    #[arg(long, default_value = "./labels_52.toml")]
    config_relabel: PathBuf,

    /// Directory containing label .txt files. Defaults to `data_dir`.
    #[arg(long)]
    labels_dir: Option<PathBuf>,

    data_dir: PathBuf,
}

#[derive(ValueEnum, Debug, Clone)]
enum DataFormat {
    YoloBBox,
    BBoxCardPose,
}

#[derive(Parser, Debug)]
struct LabelArgs {
    #[arg(long, value_enum, default_value_t = DataFormat::YoloBBox)]
    data_format: DataFormat,

    /// Directory containing images (.jpg/.jpeg/.png).
    #[arg(short, long)]
    data_dir: PathBuf,

    /// Directory containing label .txt files. Defaults to `data_dir` (labels next to images).
    #[arg(short, long)]
    labels_dir: Option<PathBuf>,

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

    #[arg(short, long)]
    start: Option<usize>,
}

type AppBuilder = eframe::AppCreator<'static>;

fn main() {
    env_logger::init();
    let args = Args::parse();

    match args.command {
        Command::Diffs(args) => {
            let labels_dir = args.labels_dir.clone().unwrap_or_else(|| args.data_dir.clone());
            check_differences(
                args.data_dir,
                labels_dir,
                &args.prefix,
                &args.prefix_relabel,
                args.config,
                args.config_relabel,
            )
            .unwrap();
        }
        Command::Label(args) => {
            let options = eframe::NativeOptions {
                ..Default::default()
            };

            let app_builder: AppBuilder = match args.data_format {
                DataFormat::YoloBBox => Box::new(|cc: &CreationContext<'_>| {
                    Ok(Box::new(BoundrsV2::<YoloBBox>::new(cc, args)) as Box<dyn eframe::App>)
                }),
                DataFormat::BBoxCardPose => Box::new(|cc: &CreationContext<'_>| {
                    Ok(Box::new(BoundrsV2::<BBoxCardPose>::new(cc, args)) as Box<dyn eframe::App>)
                }),
            };
            eframe::run_native("eframe template", options, app_builder).unwrap();
        }
        Command::FindShady(args) => {
            let shady = ShadyFinder::new(&args.data_dir).unwrap();
            shady.suggest_labels("suggested_").unwrap();
        }
    }
}

struct Tools<D: BoundrsDetection> {
    all_modes: Vec<Box<dyn Tool<D>>>,
    current_mode: usize,
    tagging: Tagging,
    tagging_active: bool,
}

impl<D: BoundrsDetection + 'static> Tools<D> {
    fn active_mut(&mut self) -> &mut dyn Tool<D> {
        if self.tagging_active {
            return &mut self.tagging;
        }
        return self.all_modes[self.current_mode].as_mut();
    }
    fn active(&self) -> &dyn Tool<D> {
        if self.tagging_active {
            return &self.tagging;
        }
        return self.all_modes[self.current_mode].as_ref();
    }
    fn cylce(&mut self) {
        self.current_mode = (self.current_mode + 1) % self.all_modes.len()
    }
    fn toggle_tagging(&mut self) {
        self.tagging_active = !self.tagging_active
    }

    fn init(args: LabelArgs, dataset: &BoundrsDataset<D>) -> Tools<D> {
        let mut all_tools: Vec<Box<dyn Tool<D>>> = vec![];
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

        let tagging = Tagging::default();
        // all_tools.push(Box::new(tagging));

        Tools {
            all_modes: all_tools,
            current_mode: 0,
            tagging,
            tagging_active: false,
        }
    }
}

struct BoundrsV2<D: BoundrsDetection> {
    // current_tool: Box<dyn Tool>,
    // tagging: TaggingTool,
    tools: Tools<D>,
    dataset: BoundrsDataset<D>,
    img_needs_focus: bool,
    // conflicts: Conflicts,
}

impl<D: BoundrsDetection + 'static> BoundrsV2<D> {
    fn new(cc: &CreationContext, args: LabelArgs) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_visuals(egui::Visuals {
            image_loading_spinners: false,
            ..Default::default()
        });
        let labels_dir = args.labels_dir.clone().unwrap_or_else(|| args.data_dir.clone());
        let mut dataset =
            BoundrsDataset::with_prefix_in_dirs(&args.data_dir, &labels_dir, &args.prefix)
                .unwrap();
        if let Some(start) = args.start {
            dataset.go(DatasetMovement::JumpTo(start), None).unwrap();
        }
        // egui_extras::install_image_loaders(ctx);
        // ctx.add_image_loader(std::sync::Arc::new(
        //     egui_extras::image_loader::ImageCrateLoader::default(),
        // ));
        // log::trace!("installed ImageCrateLoader");
        // let loaders = ctx.loaders();
        // let loaders = loaders.texture.lock();
        // for loader in loaders.iter() {
        //     println!("{:?}", loader.id());
        // }
        ctx.add_image_loader(std::sync::Arc::new(ImageCrateLoader::default()));
        log::trace!("installed ImageCrateLoader");
        ctx.add_bytes_loader(std::sync::Arc::new(BlockingFileLoader::default()));
        log::trace!("installed BlockingFileLoader");
        // let loaders = ctx.loaders();
        // let mut loaders = loaders.texture.lock();
        // loaders.clear();
        // // let texture_loader = loaders.first().unwrap();
        // drop(loaders);
        // ctx.add_texture_loader(std::sync::Arc::new(BoundrsTextureLoader::default()));
        // log::trace!("installed BlockingFileLoader");

        // let label = Labeling::new();
        let tools = Tools::init(args, &dataset);
        BoundrsV2 {
            dataset,
            tools,
            img_needs_focus: true,
        }
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
        // let ctx = ui.ctx().clone();
        // ctx.texture_ui(ui)
    }
}

impl<D: BoundrsDetection + 'static> eframe::App for BoundrsV2<D> {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Handle global shortcuts
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Space)) {
            let current = self.dataset.current();
            self.tools.active().save_state(current).unwrap();
            self.tools.cylce();
            self.tools.active_mut().refresh_state(current).unwrap();
            self.img_needs_focus = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::T)) {
            let current = self.dataset.current();
            self.tools.active().save_state(current).unwrap();
            self.tools.toggle_tagging();
            self.tools.active_mut().refresh_state(current).unwrap();
        }

        // draw own ui
        let res = egui::Window::new("Boundrs").show(&ctx, |ui| {
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
        let loaders = ctx.loaders();
        let loaders = loaders.texture.lock();
        let texture_loader = loaders.first().unwrap();
        let megas = texture_loader.byte_size() / 1_000_000;
        drop(loaders);

        if megas > 20 {
            println!("texture bytes {} Mb", megas);
            ctx.forget_all_images();
        };

        // draw central panel with image
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show_inside(ui, |ui| {
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
