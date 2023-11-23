use eframe::{egui, CreationContext};
use egui::*;
use std::path::PathBuf;

use boundrs::dataset::DynLabelConfig;

use boundrs::conflicts::Conflicts;
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
#[derive(PartialEq, Debug)]
enum Mode {
    Label,
    Relabel,
    Conflicts,
}

fn main() {
    let args = Args::parse();
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920.0, 1080.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Show an image with eframe/egui",
        options,
        Box::new(|cc| Box::new(Boundrs::new(cc, args))),
    )
    .unwrap();
}

struct Boundrs {
    label: Labeling,
    relabel: Relabeling,
    conflicts: Option<Conflicts>,
    mode: Mode,
}

impl Boundrs {
    fn handle_mode_switch(&mut self, ctx: &Context) {
        // We save the current label and update the state in the new mode
        if ctx.input(|i| i.key_pressed(Key::Tab)) {
            // TODO this desperately needs to be refactored. Maybe label and relabel as tools (not owning their datatsets), instead of apps
            self.mode = match self.mode {
                Mode::Label => {
                    let sync = self.label.prepare_switch();
                    self.relabel.refresh_after_switch(&sync, ctx);
                    Mode::Relabel
                }
                Mode::Relabel => {
                    // let (_, current_pos, _) = self.relabel.old_dataset.get_progress();
                    // TODO fix this
                    let sync = self.relabel.prepare_switch();
                    if self.conflicts.is_none() {
                        self.label.refresh_after_switch(&sync, ctx);
                        Mode::Label
                    } else {
                        self.conflicts
                            .as_mut()
                            .unwrap()
                            .refresh_after_switch(&sync, ctx);
                        Mode::Conflicts
                    }
                }
                Mode::Conflicts => {
                    let sync = self.conflicts.as_mut().unwrap().prepare_switch();
                    self.label.refresh_after_switch(&sync, ctx);
                    Mode::Label
                }
            };
        }
    }

    fn new(cc: &CreationContext, args: Args) -> Self {
        let label_config = DynLabelConfig::load_from_file(&args.config)
            .expect("./labels.toml should exists as described in github repo");
        let label_state = Labeling::new(&cc.egui_ctx, &args.data_dir, &args.prefix, label_config);
        let label_config = DynLabelConfig::load_from_file(&args.config)
            .expect("./labels.toml should exists as described in github repo");
        let relabel_config = DynLabelConfig::load_from_file(&args.config_relabel)
            .expect("./labels.toml should exists as described in github repo");
        let relabel_state = Relabeling::new(
            &cc.egui_ctx,
            &args.data_dir,
            &args.prefix,
            &args.prefix_relabel,
            label_config,
            relabel_config,
        );

        let conflicts = match args.conflicts_dir {
            Some(dir) => Some(Conflicts::from_dir(&cc.egui_ctx, &dir).unwrap()),
            None => None,
        };

        Self {
            label: label_state,
            relabel: relabel_state,
            conflicts,
            mode: Mode::Label,
        }
    }
}

impl eframe::App for Boundrs {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::Window::new("Boundrs Labeling").show(ctx, |ui| {
            // ui.horizontal(|ui| {
            //     ui.selectable_value(&mut self.mode, Mode::Label, "Label");
            //     ui.selectable_value(&mut self.mode, Mode::Relabel, "Relabel");
            // });
            ui.label(format!("Current mode: {:?}", self.mode));
            ui.separator();

            match self.mode {
                Mode::Label => self.label.draw_ui(ui, ctx),
                Mode::Relabel => self.relabel.draw_ui(ui, ctx),
                Mode::Conflicts => self.conflicts.as_mut().unwrap().draw_ui(ui, ctx),
            }
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                // Draw image

                self.handle_mode_switch(ctx);

                match self.mode {
                    Mode::Label => self.label.draw_central_panel(ctx, ui),
                    Mode::Relabel => self.relabel.draw_central_panel(ctx, ui),
                    Mode::Conflicts => self.conflicts.as_mut().unwrap().draw_central_panel(ctx, ui), // TODO refactor. The mode contains the ref to the trait object? unwrap because we dont enter this mode if not available
                }
            });
    }
}
