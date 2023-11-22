use anyhow::Result;
use eframe::{egui, CreationContext};
use egui::*;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use boundrs::dataset::{Dataset, DatasetMovement, DynLabel, DynLabelConfig, YoloBB, YoloLabel};
use image::{Rgba, RgbaImage};

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
}
#[derive(PartialEq, Debug)]
enum Mode {
    Label,
    Relabel,
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

#[derive(Debug, Clone, Copy)]
enum BBoxInput {
    None,
    Partial(Pos2),
    Finished(Pos2, Pos2),
}

struct Labeling {
    dataset: Dataset,
    label_config: DynLabelConfig,
    zoom: f32,
    // TODO move this to main app, pass new texture out of label / relabel function
    image_texture: egui::TextureHandle,
    mask_texture: egui::TextureHandle,
    img_rect: Rect,
    bbox_input: BBoxInput,
    repeat_mode: bool,
    current_class: DynLabel,
    filter: bool,
    filter_opacity: u8,
    shown_classes: HashSet<usize>,
    last_time: f64,
    current_label: YoloLabel,
}

impl Labeling {
    fn new(cc: &Context, dataset_dir: &Path, prefix: &str, label_config: DynLabelConfig) -> Self {
        let shown_classes = HashSet::new();
        let dataset = Dataset::with_prefix(dataset_dir, prefix).unwrap();
        let image = dataset.current_image().unwrap();
        let image_texture = cc.load_texture("my-image", image, egui::TextureOptions::LINEAR);
        let current_bbs = dataset.current_label().unwrap();
        let current_class = label_config
            .label_from_usize(0)
            .expect("At least 1 label is needed");
        let mask = generate_mask(&current_bbs, &shown_classes, Rect::NOTHING, 250);
        let mask_texture = cc.load_texture("mask", mask, egui::TextureOptions::LINEAR);
        Labeling {
            dataset,
            label_config,
            zoom: 1.5,
            image_texture,
            mask_texture,
            img_rect: Rect::NOTHING,
            bbox_input: BBoxInput::None,
            repeat_mode: false,
            current_class,
            filter: false,
            filter_opacity: 250,
            shown_classes: HashSet::new(),
            last_time: 0.0,
            current_label: current_bbs,
        }
    }
    fn draw_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        let filename = self.dataset.current_name();
        ui.horizontal(|ui| {
            ui.label("Current image:");
            ui.label(filename);
        });
        ui.horizontal(|ui| {
            ui.label("Progress");
            ui.add(DragValue::from_get_set(|new_pos| {
                if let Some(new_pos) = new_pos {
                    self.dataset_move(DatasetMovement::JumpTo(new_pos as usize), ctx);
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
        ui.horizontal(|ui| {
            ui.label("Filter opacity");
            ui.add(Slider::new(&mut self.filter_opacity, 0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("Shown classes:");
            // TODO sort this by enum order and properly implement display or smth
            let mut classes: Vec<String> = self
                .shown_classes
                .iter()
                .map(|u| self.label_config.label_from_usize(*u).unwrap())
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
        let image = self.dataset.current_image().unwrap();
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
    pub fn remove_labels(&mut self, pos: Pos2, img_rect: Rect) {
        self.current_label
            .retain(|label| !label.to_screen_rect(img_rect).contains(pos));
    }
    pub fn add_bb(&mut self, bb: YoloBB) {
        self.current_label.push(bb)
    }

    pub fn repeat_bbs_inside(&mut self, rect: Rect) -> Result<()> {
        // get two coordinates to repeat only the labels completely in this box
        let prev_label = self.dataset.previous_labels(1)?[0].clone();
        let prev_label_inside = prev_label
            .into_iter()
            .filter(|l| rect.contains_rect(l.to_screen_rect(self.img_rect)));
        self.current_label = self
            .current_label
            .clone()
            .into_iter()
            .filter(|l| !rect.contains_rect(l.to_screen_rect(self.img_rect)))
            .chain(prev_label_inside)
            .collect();
        Ok(())
    }
    fn remove_bbs(&mut self, pos: Pos2, img_rect: Rect) {
        self.remove_labels(pos, img_rect);
    }
    fn draw_bbs(&self, ui: &mut Ui) {
        let img_rect = self.img_rect;
        let painter = ui.painter();
        // let size = self.img_rect.size();
        for bb in &self.current_label {
            let color = bb.class(&self.label_config).color;
            let screen_rect = bb.to_screen_rect(img_rect);
            painter.rect_stroke(screen_rect, Rounding::none(), Stroke::new(2.0, color));
            let text_pos = screen_rect.left_bottom();
            self.draw_label_text(painter, text_pos, &bb.class(&self.label_config));
        }
    }
    fn draw_guide(&self, ui: &mut Ui, pos: Pos2) {
        let painter = ui.painter();
        let rect = ui.clip_rect();
        let w_size = rect.size();
        let color = self.current_class.color;
        let stroke = egui::Stroke::new(2.0, color);
        painter.hline(0.0..=w_size.x, pos.y, stroke);
        painter.vline(pos.x, 0.0..=w_size.y, stroke);
        self.draw_label_text(painter, pos, &self.current_class);
    }
    fn draw_partial_box(&self, ui: &mut Ui) {
        if let BBoxInput::Partial(pos) = self.bbox_input {
            // let screen_pos = self.img_to_screen_coordinates(pos);
            self.draw_guide(ui, pos);
        }
    }
    fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: &DynLabel) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::none(),
            class.color,
            Stroke::NONE,
        );
        let text = if self.repeat_mode {
            "Repeat region"
        } else {
            &class.name
        };
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
            let screen_pos = img_response.interact_pointer_pos().unwrap();
            self.remove_bbs(screen_pos, img_response.rect);
            self.update_mask(ui.ctx());
        }

        // secondary click also regiesters a drag, therefore early return
        if ui.input(|i| i.pointer.button_down(PointerButton::Secondary)) {
            return;
        }
        self.bbox_input = match self.bbox_input {
            BBoxInput::None if img_response.drag_started() => {
                let screen_pos = img_response.interact_pointer_pos().unwrap();
                BBoxInput::Partial(screen_pos)
            }
            BBoxInput::None => BBoxInput::None,
            BBoxInput::Partial(pos1) if img_response.drag_released() => {
                let pos2 = img_response.interact_pointer_pos().unwrap();
                // sometimes you drag a tiny amount without wanting to
                if (pos2.x - pos1.x).abs() < 20.0 || (pos2.y - pos1.y).abs() < 20.0 {
                    BBoxInput::Partial(pos1)
                } else {
                    BBoxInput::Finished(pos1, pos2)
                }
            }
            BBoxInput::Partial(pos1) => BBoxInput::Partial(pos1),
            BBoxInput::Finished(pos1, pos2) => {
                let img_rect = img_response.rect;
                let label_rect = Rect::from_two_pos(pos1, pos2);
                let label = YoloBB::from_rect(label_rect, img_rect, &self.current_class);
                println!("{label:?}");
                self.add_bb(label);
                self.update_mask(ui.ctx());
                BBoxInput::None
            }
        };
        if let BBoxInput::Finished(pos1, pos2) = self.bbox_input {
            if self.repeat_mode {
                let rect = Rect::from_two_pos(pos1, pos2);
                self.repeat_bbs_inside(rect).unwrap();
                self.repeat_mode = false;
                self.bbox_input = BBoxInput::None
            } else {
                let img_rect = img_response.rect;
                let label_rect = Rect::from_two_pos(pos1, pos2);
                let label = YoloBB::from_rect(label_rect, img_rect, &self.current_class);
                println!("{label:?}");
                self.add_bb(label);
                self.update_mask(ui.ctx());
                self.bbox_input = BBoxInput::None
            }
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
        if ctx.input(|i| i.time - self.last_time > 0.3) {
            return ctx.input(|i| self.label_config.label_from_keys(&i.keys_down));
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
                self.current_class = class.clone();
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
        self.dataset
            .go(movement, self.current_label.clone(), true)
            .unwrap();
        self.current_label = self.dataset.current_label().unwrap();
        self.update_texture(ctx);
        self.update_mask(ctx);
    }
}

struct Boundrs {
    label: Labeling,
    relabel: Relabeling,
    mode: Mode,
}

impl Boundrs {
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
        if ctx.input(|i| i.key_pressed(Key::Tab)) {
            // TODO this desperately needs to be refactored. Maybe label and relabel as tools (not owning their datatsets), instead of apps
            self.mode = match self.mode {
                Mode::Label => {
                    let (_, current_pos, _) = self.label.dataset.get_progress();
                    let label_move = DatasetMovement::JumpTo(current_pos);
                    let relabel_move = DatasetMovement::JumpTo(current_pos);
                    self.relabel.go(relabel_move, ctx); // move relabel dataset

                    // TODO fix this
                    self.label.dataset_move(label_move, ctx); // This saves currentl label to disk
                    self.relabel.old_label = self.label.current_label.clone(); // override relabels old_label
                    self.relabel.highlighted = self.relabel.find_next_highlighted(); // If we removed the highlighted, it would crash if we dont update this... refactor
                    Mode::Relabel
                }
                Mode::Relabel => {
                    // let (_, current_pos, _) = self.relabel.old_dataset.get_progress();
                    // TODO fix this
                    let (_, current_pos, _) = self.relabel.old_dataset.get_progress();
                    let label_move = DatasetMovement::JumpTo(current_pos);
                    let movement = DatasetMovement::JumpTo(current_pos);
                    self.label.dataset_move(label_move, ctx); // move label dataset
                    self.relabel.go(movement, ctx); // save relabels old_label and new_label

                    self.label.current_label = self.relabel.old_label.clone(); // override labels current_label
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

        Self {
            label: label_state,
            relabel: relabel_state,
            mode: Mode::Label,
        }
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
            }
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                // Draw image

                self.handle_mode_switch(ctx);

                match self.mode {
                    Mode::Label => {
                        let app = &mut self.label;

                        let img_response = app.draw_img(ui);

                        // filter
                        if app.filter {
                            ui.put(
                                app.img_rect,
                                egui::Image::new(&app.mask_texture, app.mask_texture.size_vec2()),
                            );
                        }

                        // Draw guides
                        let pos = ctx.input(|i| i.pointer.hover_pos());
                        if let Some(pos) = pos {
                            app.draw_guide(ui, pos)
                        }
                        app.draw_partial_box(ui);

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

                        // Handle repeat button
                        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
                            app.repeat_mode = !app.repeat_mode;
                        }
                    }
                    Mode::Relabel => {
                        let app = &mut self.relabel;

                        let img_response = app.draw_img(ui);

                        // Handle right click
                        app.handle_img_response(img_response, ui);

                        // Draw bbs
                        app.draw_bbs(ui);

                        app.draw_highlight(ui);

                        // Handle prev next picture keyboard
                        app.handle_left_right(ctx);

                        // Handle class setting
                        app.handle_class_keys(ctx);

                        // Handle labels clearing
                        app.handle_clear(ctx);

                        // Handle repeat button
                        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
                            app.repeat_bbs().unwrap();
                        }
                    }
                }
            });
    }
}
