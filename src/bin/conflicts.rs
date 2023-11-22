use anyhow::Result;
use eframe::{egui, CreationContext};
use egui::*;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
};

use boundrs::dataset::{Dataset, DatasetMovement, DynLabel, DynLabelConfig, YoloBB, YoloLabel};
use image::{Rgba, RgbaImage};

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    gt_dir: PathBuf,

    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    pred_dirs: Vec<PathBuf>,

    #[arg(long, default_value = "./labels_52.toml")]
    gt_config: PathBuf,

    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    configs_pred: Vec<PathBuf>,
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
        Box::new(|cc| Box::new(BoundrsConflicts::new(cc, args))),
    )
    .unwrap();
}

#[derive(Debug)]
enum CurrentDataset {
    GroundTruth,
    Predicted(usize),
}

struct Conflicts {
    gt_dataset: Dataset,
    gt_config: DynLabelConfig,
    // TODO store just the folder paths here and config in a separate dictionary
    pred_dirs: Vec<PathBuf>,
    pred_confs: Vec<DynLabelConfig>,
    zoom: f32,
    // TODO move this to main app, pass new texture out of label / relabel function
    image_texture: egui::TextureHandle,
    mask_texture: egui::TextureHandle,
    img_rect: Rect,
    filter: bool,
    filter_opacity: u8,
    last_time: f64,
    shown_classes: HashSet<usize>,
    current_dataset: CurrentDataset,
    current_label: YoloLabel,
}

impl Conflicts {
    fn new(
        cc: &Context,
        gt_dataset_dir: &Path,
        gt_config: DynLabelConfig,
        pred_dirs: Vec<PathBuf>,
        pred_confs: Vec<DynLabelConfig>,
    ) -> Self {
        let shown_classes = HashSet::new();
        let gt_dataset = Dataset::from_input_dir(gt_dataset_dir).unwrap();
        let image = gt_dataset.current_image().unwrap();
        let image_texture = cc.load_texture("my-image", image, egui::TextureOptions::LINEAR);
        let current_bbs = gt_dataset.current_label().unwrap();
        let mask = generate_mask(&current_bbs, &shown_classes, Rect::NOTHING, 250);
        let mask_texture = cc.load_texture("mask", mask, egui::TextureOptions::LINEAR);

        Conflicts {
            gt_dataset,
            gt_config,
            zoom: 1.5,
            image_texture,
            mask_texture,
            img_rect: Rect::NOTHING,
            filter: false,
            filter_opacity: 250,
            last_time: 0.0,
            shown_classes: HashSet::new(),
            current_label: current_bbs,
            pred_dirs,
            pred_confs,
            current_dataset: CurrentDataset::GroundTruth,
        }
    }
    fn get_current_dataset_mut(&mut self) -> &mut Dataset {
        &mut self.gt_dataset
    }
    fn get_current_config(&self) -> &DynLabelConfig {
        match self.current_dataset {
            CurrentDataset::GroundTruth => &self.gt_config,
            CurrentDataset::Predicted(i) => &self.pred_confs[i],
        }
    }
    fn draw_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            let filename = self.gt_dataset.current_name();
            ui.label("Current image:");
            ui.label(filename);
            let path = self.gt_dataset.current_path();
            ui.label("Current dataset:");
            ui.label(format!("{:?}", self.current_dataset));
        });
        ui.horizontal(|ui| {
            ui.label("Progress");
            ui.add(DragValue::from_get_set(|new_pos| {
                if let Some(new_pos) = new_pos {
                    self.dataset_move(DatasetMovement::JumpTo(new_pos as usize), ctx);
                }
                self.gt_dataset.get_progress().1 as f64
            }));
            let (_, current, max) = self.gt_dataset.get_progress();
            ui.add(
                ProgressBar::new(current as f32 / max as f32)
                    .show_percentage()
                    .text(format!("{current} out of {max} images")),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Filter opacity");
            ui.add(Slider::new(&mut self.filter_opacity, 0..=255));
        });
        ui.horizontal(|ui| {
            let config = self.get_current_config();
            ui.label("Shown classes:");
            // TODO sort this by enum order and properly implement display or smth
            let mut classes: Vec<String> = self
                .shown_classes
                .iter()
                .map(|u| config.label_from_usize(*u).unwrap())
                .map(|l| l.name)
                .collect();
            classes.sort();
            ui.label(format!("{classes:?}"));
        });
        ui.horizontal(|ui| {
            ui.label("Zoom image:");
            ui.add(DragValue::new(&mut self.zoom).speed(0.01));
        });
        ui.vertical(|ui| {
            ui.separator();
            ui.label(RichText::new("How to use").heading());
            ui.label(RichText::new(
                "Choose the active label as with the keybindings as in the config file",
            ));
            ui.label(RichText::new("Create label by clicking twice or dragging"));
            ui.label(RichText::new("Delete labels with a right click"));
            ui.label(RichText::new("Repeat previous labels: R"));
            ui.label(RichText::new(
                "Go left and right: Left Arrow or A and Right Arrow or D",
            ));
        });
    }
    pub fn draw_img(&mut self, ui: &mut Ui) -> Response {
        let img_response = ui.add(
            egui::Image::new(
                &self.image_texture,
                self.image_texture.size_vec2() * self.zoom,
            )
            .sense(Sense::click_and_drag()),
        );
        self.img_rect = img_response.rect;
        img_response
    }
    fn update_texture(&mut self, ctx: &Context) {
        let image = self.gt_dataset.current_image().unwrap();
        self.image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
    }
    fn update_mask(&mut self, ctx: &Context) {
        if self.filter {
            let mask = generate_mask(
                &self.current_label,
                &self.shown_classes,
                self.img_rect,
                self.filter_opacity,
            );
            self.mask_texture = ctx.load_texture("mask", mask, egui::TextureOptions::LINEAR);
        }
    }
    fn draw_bbs(&self, ui: &mut Ui) {
        let config = self.get_current_config();
        let img_rect = self.img_rect;
        let painter = ui.painter();
        // let size = self.img_rect.size();
        for bb in &self.current_label {
            let color = bb.class(config).color;
            let screen_rect = bb.to_screen_rect(img_rect);
            painter.rect_stroke(screen_rect, Rounding::none(), Stroke::new(2.0, color));
            let text_pos = screen_rect.left_bottom();
            self.draw_label_text(painter, text_pos, &bb.class(config));
        }
    }
    fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: &DynLabel) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::none(),
            class.color,
            Stroke::NONE,
        );
        let text = &class.name;
        let _text_rect = painter.text(
            text_pos,
            Align2::LEFT_BOTTOM,
            text,
            FontId::monospace(35.0),
            Color32::BLACK,
        );
    }
    fn handle_img_response(&mut self, img_response: Response, ui: &mut Ui) {
        if img_response.secondary_clicked() {
            self.update_mask(ui.ctx());
        }

        // secondary click also regiesters a drag, therefore early return
        if ui.input(|i| i.pointer.button_down(PointerButton::Secondary)) {
            return;
        }
    }
    fn class_pressed(&self, ctx: &Context) -> Option<DynLabel> {
        // for (i, keys) in self.label_config.keybindings().into_iter().enumerate() {
        //     if keys.iter().all(|k| ctx.input(|i| i.key_down(*k))) {
        //         // consume all keys
        //         keys.iter()
        //             .all(|key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, *key)));
        //         let label = self.label_config.label_from_usize(i).unwrap();
        //         return Some(label);
        //     }
        // }
        // None

        let config = self.get_current_config();
        if ctx.input(|i| i.time - self.last_time > 0.3) {
            return ctx.input(|i| config.label_from_keys(&i.keys_down));
        }
        None
    }

    fn handle_class_keys(&mut self, ctx: &Context) {
        if let Some(class) = self.class_pressed(ctx) {
            self.last_time = ctx.input(|i| i.time);
            if self.filter {
                if self.shown_classes.contains(&class.i) {
                    self.shown_classes.remove(&class.i);
                } else {
                    self.shown_classes.insert(class.i);
                }
                self.update_mask(ctx);
            } else {
            }
        }
    }
    fn handle_left_right(&mut self, ctx: &Context) {
        let next_pressed =
            ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) | i.key_pressed(egui::Key::D));
        let previous_pressed =
            ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) | i.key_pressed(egui::Key::A));

        let movement = match (next_pressed, previous_pressed, self.filter) {
            (true, false, false) => DatasetMovement::Next,
            (false, true, false) => DatasetMovement::Previous,
            (true, false, true) => DatasetMovement::NextContaining(self.shown_classes.clone()),
            (false, true, true) => DatasetMovement::PreviousContaining(self.shown_classes.clone()),
            _ => return,
        };
        self.dataset_move(movement, ctx);
    }

    fn dataset_move(&mut self, movement: DatasetMovement, ctx: &Context) {
        let current_label = self.current_label.clone();

        // move gt dataset
        self.gt_dataset
            .go(movement.clone(), current_label.clone(), false)
            .unwrap();
        self.current_label = self.gt_dataset.current_label().unwrap();
        self.current_dataset = CurrentDataset::GroundTruth; // otherwise need to find current_label in special way
        self.update_texture(ctx);
        self.update_mask(ctx);
    }
}

struct BoundrsConflicts {
    conflicts: Conflicts,
}

impl BoundrsConflicts {
    // TODO error handling
    // fn build_app(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
    //     let label_state = Labeling::new(&cc.egui_ctx);
    //     let relabel_state = Relabeling::new(&cc.egui_ctx);

    //     Box::new(Self {
    //         label: label_state,
    //         relabel: relabel_state,
    //         mode: Mode::Label,
    //     })
    // }

    fn handle_mode_switch(&mut self, ctx: &Context) {
        // We save the current label and update the state in the new mode
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            // TODO this desperately needs to be refactored. Maybe label and relabel as tools (not owning their datatsets), instead of apps

            let mut existing_pred_indices = vec![0];
            for (i, path) in self.conflicts.pred_dirs.iter().enumerate() {
                let mut gt_name: PathBuf = self.conflicts.gt_dataset.current_name().into();
                gt_name.set_extension("txt");
                let mut label_file = path.clone();
                label_file.push(&gt_name);
                println!("checking {:?}", label_file);
                if label_file.exists() {
                    existing_pred_indices.push(i + 1) // to cycle through them, I put gt to 0
                }
            }
            println!("{:?}", existing_pred_indices);

            let current_index = match &self.conflicts.current_dataset {
                CurrentDataset::GroundTruth => 0,
                CurrentDataset::Predicted(i) => i + 1,
            };
            let closest_index = existing_pred_indices
                .iter()
                .min_by_key(|&&i| current_index.abs_diff(i))
                .unwrap();
            let pos_closest_index = existing_pred_indices
                .iter()
                .position(|i| i == closest_index)
                .unwrap();
            let new_pos = (pos_closest_index + 1) % existing_pred_indices.len();
            let new_pos = existing_pred_indices[new_pos];

            if new_pos == 0 {
                self.conflicts.current_dataset = CurrentDataset::GroundTruth;
            } else {
                self.conflicts.current_dataset = CurrentDataset::Predicted(new_pos - 1);
            }

            // search through the dirs to find a conflicting file
            // TODO generalize this code and move it into Conflicts
            // TODO then the search can be made by recorsively calling this code
            let label = match self.conflicts.current_dataset {
                CurrentDataset::GroundTruth => self.conflicts.gt_dataset.current_label().unwrap(),
                CurrentDataset::Predicted(i) => {
                    // Here we know the file exists because of the code above
                    let mut gt_name: PathBuf = self.conflicts.gt_dataset.current_name().into();
                    gt_name.set_extension("txt");
                    let path = &self.conflicts.pred_dirs[i];
                    let mut label_file = path.clone();
                    label_file.push(gt_name);
                    assert!(label_file.exists());
                    let yolo_strs = std::fs::read_to_string(label_file).unwrap();

                    let mut labels = vec![];
                    for line in yolo_strs.lines() {
                        let label = YoloBB::from_str(line).unwrap();
                        labels.push(label)
                    }
                    labels
                }
            };
            self.conflicts.current_label = label;
        }
    }

    fn new(cc: &CreationContext, args: Args) -> Self {
        let gt_config = DynLabelConfig::load_from_file(&args.gt_config)
            .expect("./labels.toml should exists as described in github repo");
        let pred_configs = args
            .configs_pred
            .into_iter()
            .map(|path| {
                DynLabelConfig::load_from_file(&path)
                    .expect("./labels.toml should exists as described in github repo")
            })
            .collect();
        let conflicts = Conflicts::new(
            &cc.egui_ctx,
            &args.gt_dir,
            gt_config,
            args.pred_dirs,
            pred_configs,
        );

        Self { conflicts }
    }
}

#[inline]
fn pos_inside_label_box(label: &YoloLabel, pos: Pos2, img_rect: Rect) -> bool {
    label
        .iter()
        .any(|l| l.to_screen_rect(img_rect).contains(pos))
}
fn generate_mask(
    label: &YoloLabel,
    shown_classes: &HashSet<usize>,
    img_rect: Rect,
    opacity: u8,
) -> ColorImage {
    let highlighted_label = label
        .iter()
        .cloned()
        .filter(|bb| shown_classes.contains(&bb.class_num))
        .collect();
    let width = img_rect.width() as usize;
    let height = img_rect.height() as usize;
    let img_rect = img_rect.translate(-img_rect.left_top().to_vec2());
    let mask = RgbaImage::from_fn(width as u32, height as u32, |x, y| {
        let pos = Pos2::new(x as f32, y as f32);
        if pos_inside_label_box(&highlighted_label, pos, img_rect) {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([0, 0, 0, opacity])
        }
    });
    let pixels = mask.as_flat_samples();
    ColorImage::from_rgba_unmultiplied([width, height], pixels.as_slice())
}

impl eframe::App for BoundrsConflicts {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::Window::new("Boundrs Labeling").show(ctx, |ui| {
            // ui.horizontal(|ui| {
            //     ui.selectable_value(&mut self.mode, Mode::Label, "Label");
            //     ui.selectable_value(&mut self.mode, Mode::Relabel, "Relabel");
            // });
            // ui.label(format!("Current mode: {:?}", self.mode));
            // ui.separator();

            self.conflicts.draw_ui(ui, ctx);
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                // Draw image

                self.handle_mode_switch(ctx);

                let app = &mut self.conflicts;

                let img_response = app.draw_img(ui);

                // filter
                if app.filter {
                    ui.put(
                        app.img_rect,
                        egui::Image::new(&app.mask_texture, app.mask_texture.size_vec2()),
                    );
                }

                // Draw bbs
                app.draw_bbs(ui);

                // Handle prev next picture keyboard
                app.handle_left_right(ctx);

                // Handle clicks for bbs
                app.handle_img_response(img_response, ui);
                // Handle class setting
                app.handle_class_keys(ctx);
                // Handle filter mode
                let filter_pressed = ctx.input(|i| i.key_pressed(egui::Key::F));
                if filter_pressed {
                    app.filter = !app.filter;
                }
            });
    }
}
