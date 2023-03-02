use anyhow::Result;
use eframe::egui;
use egui::*;
use std::collections::HashSet;

mod dataset;
use dataset::{BoundingBox, Dataset, DatasetLabel, DatasetMovement, DynLabel, YoloBB, YoloLabel};
use image::{Rgba, RgbaImage};

mod relabeling;
use relabeling::Relabeling;

#[derive(PartialEq, Debug)]
enum Mode {
    Label,
    Relabel,
}

fn main() {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920.0, 1080.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Show an image with eframe/egui",
        options,
        Box::new(Boundrs::build_app),
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
    dataset: Dataset<DynLabel>,
    zoom: f32,
    // TODO move this to main app, pass new texture out of label / relabel function
    image_texture: egui::TextureHandle,
    mask_texture: egui::TextureHandle,
    img_rect: Rect,
    bbox_input: BBoxInput,
    current_class: DynLabel,
    filter: bool,
    filter_opacity: u8,
    shown_classes: HashSet<DynLabel>,
    current_label: YoloLabel<DynLabel>,
}

impl Labeling {
    fn new(cc: &Context) -> Self {
        let ppp = cc.pixels_per_point();
        println!("Current pixels per point {ppp}");
        // cc.set_pixels_per_point(2.0);
        let shown_classes = HashSet::new();
        let dataset = Dataset::from_input_dir().unwrap();
        let image = dataset.current_image().unwrap();
        let image_texture = cc.load_texture("my-image", image, egui::TextureOptions::LINEAR);
        let current_bbs = dataset.current_label().unwrap();
        let mask = generate_mask(&current_bbs, &shown_classes, Rect::NOTHING, 250);
        let mask_texture = cc.load_texture("mask", mask, egui::TextureOptions::LINEAR);
        Labeling {
            dataset,
            zoom: 1.0,
            image_texture,
            mask_texture,
            img_rect: Rect::NOTHING,
            bbox_input: BBoxInput::None,
            current_class: DynLabel(0),
            filter: false,
            filter_opacity: 250,
            shown_classes: HashSet::new(),
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
            ui.label(format!("{:?}", self.shown_classes));
        });
        ui.horizontal(|ui| {
            ui.label("Zoom image:");
            ui.add(DragValue::new(&mut self.zoom).speed(0.01));
        });
        ui.vertical(|ui| {
            ui.separator();
            ui.label(RichText::new("How to use").heading());
            ui.label(RichText::new("Choos the active label with:"));
            ui.label(RichText::new("1: Ace"));
            ui.label(RichText::new("2: 2"));
            ui.label(RichText::new("..."));
            ui.label(RichText::new("0: 10"));
            ui.label(RichText::new("J: J"));
            ui.label(RichText::new("Q: Q"));
            ui.label(RichText::new("K: K"));
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
    pub fn add_bb(&mut self, bb: YoloBB<DynLabel>) {
        self.current_label.push(bb)
    }

    pub fn repeat_bbs(&mut self) -> Result<()> {
        let yolo_label = self.dataset.previous_label()?;
        self.current_label = yolo_label;
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
            let color = bb.class().color();
            let screen_rect = bb.to_screen_rect(img_rect);
            painter.rect_stroke(screen_rect, Rounding::none(), Stroke::new(2.0, color));
            let text_pos = screen_rect.left_bottom();
            self.draw_label_text(painter, text_pos, bb.class());
        }
    }
    fn draw_guide(&self, ui: &mut Ui, pos: Pos2) {
        let painter = ui.painter();
        let rect = ui.clip_rect();
        let w_size = rect.size();
        let color = self.current_class.color();
        let stroke = egui::Stroke::new(2.0, color);
        painter.hline(0.0..=w_size.x, pos.y, stroke);
        painter.vline(pos.x, 0.0..=w_size.y, stroke);
        self.draw_label_text(painter, pos, self.current_class);
    }
    fn draw_partial_box(&self, ui: &mut Ui) {
        if let BBoxInput::Partial(pos) = self.bbox_input {
            // let screen_pos = self.img_to_screen_coordinates(pos);
            self.draw_guide(ui, pos);
        }
    }
    fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: DynLabel) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::none(),
            class.color(),
            Stroke::NONE,
        );
        let _text_rect = painter.text(
            text_pos,
            Align2::LEFT_BOTTOM,
            class.to_name(),
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
                if (pos2.x - pos1.x).abs() < 5.0 || (pos2.y - pos1.y).abs() < 5.0 {
                    BBoxInput::Partial(pos1)
                } else {
                    BBoxInput::Finished(pos1, pos2)
                }
            }
            BBoxInput::Partial(pos1) => BBoxInput::Partial(pos1),
            BBoxInput::Finished(pos1, pos2) => {
                let class = self.current_class;
                let img_rect = img_response.rect;
                let label_rect = Rect::from_two_pos(pos1, pos2);
                let label = YoloBB::from_rect(label_rect, img_rect, class);
                println!("{label:?}");
                self.add_bb(label);
                self.update_mask(ui.ctx());
                BBoxInput::None
            }
        };
    }
    fn class_pressed(&self, ctx: &Context) -> Option<DynLabel> {
        ctx.input(|i| DynLabel::keys_to_class(i.keys_down.clone()))
    }

    fn handle_class_keys(&mut self, ctx: &Context) {
        if let Some(class) = self.class_pressed(ctx) {
            if self.filter {
                if self.shown_classes.contains(&class) {
                    self.shown_classes.remove(&class);
                } else {
                    self.shown_classes.insert(class);
                }
            } else {
                self.current_class = class;
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

    fn dataset_move(&mut self, movement: DatasetMovement<DynLabel>, ctx: &Context) {
        self.dataset
            .go(movement, self.current_label.clone())
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
    fn build_app(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
        let label_state = Labeling::new(&cc.egui_ctx);
        let relabel_state = Relabeling::new(&cc.egui_ctx);

        Box::new(Self {
            label: label_state,
            relabel: relabel_state,
            mode: Mode::Label,
        })
    }

    fn handle_mode_switch(&mut self, ctx: &Context) {
        // We save the current label and update the state in the new mode
        if ctx.input(|i| i.key_pressed(Key::Tab)) {
            self.mode = match self.mode {
                Mode::Label => {
                    let (_, current_pos, _) = self.label.dataset.get_progress();
                    let label_move = DatasetMovement::JumpTo(current_pos);
                    self.label.dataset_move(label_move, ctx);
                    let relabel_old_move = DatasetMovement::JumpTo(current_pos);
                    let relabel_new_move = DatasetMovement::JumpTo(current_pos);
                    self.relabel.go(relabel_old_move, relabel_new_move, ctx);
                    // self.relabel.old_label = self.label.current_label;
                    Mode::Relabel
                }
                Mode::Relabel => {
                    // let (_, current_pos, _) = self.relabel.old_dataset.get_progress();
                    Mode::Label
                }
            };
        }
    }
}

#[inline]
fn pos_inside_label_box(label: &YoloLabel<DynLabel>, pos: Pos2, img_rect: Rect) -> bool {
    label
        .iter()
        .any(|l| l.to_screen_rect(img_rect).contains(pos))
}
fn generate_mask(
    label: &YoloLabel<DynLabel>,
    shown_classes: &HashSet<DynLabel>,
    img_rect: Rect,
    opacity: u8,
) -> ColorImage {
    let highlighted_label = label
        .iter()
        .cloned()
        .filter(|bb| shown_classes.contains(&bb.class()))
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
                            app.repeat_bbs().unwrap();
                        }
                    }
                    Mode::Relabel => {
                        let app = &mut self.relabel;

                        app.draw_img(ui);

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
