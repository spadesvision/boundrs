use anyhow::Result;
use clap::{Parser, Subcommand};
use eframe::egui;
use egui::*;
use std::collections::HashSet;

mod dataset;
use dataset::{BoundingBox, Card, Dataset, DatasetMovement, Label, YoloBB, YoloLabel};
use image::{Rgba, RgbaImage};

mod relabeling;
use relabeling::Relabeling;

#[derive(Subcommand)]
enum Mode {
    Label,
    Relabel,
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

// Here is a simplified version of the code:

fn main() {
    let cli = Cli::parse();
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1920.0, 1080.0)),
        ..Default::default()
    };

    let app = match cli.mode {
        Mode::Label => Box::new(Boundrs::build_app) as eframe::AppCreator,
        Mode::Relabel => Box::new(Relabeling::build_app) as eframe::AppCreator,
    };

    eframe::run_native("Show an image with eframe/egui", options, app);
}

#[derive(Debug, Clone, Copy)]
enum BBoxInput {
    None,
    Partial(Pos2),
    Finished(Pos2, Pos2),
}

struct Boundrs {
    image_texture: egui::TextureHandle,
    mask_texture: egui::TextureHandle,
    bbox_input: BBoxInput,
    dataset: Dataset<Card>,
    current_class: Card,
    image_rect: Rect,
    filter: bool,
    filter_opacity: u8,
    shown_classes: HashSet<Card>,
    current_label: YoloLabel<Card>,
}

impl Boundrs {
    // TODO error handling
    fn build_app(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
        let dataset = Dataset::from_input_dir().unwrap();
        let image = dataset.current_image().unwrap();
        let image_texture =
            cc.egui_ctx
                .load_texture("my-image", image, egui::TextureFilter::Linear);
        let size = image_texture.size_vec2();
        let current_bbs = dataset.current_label().unwrap();
        let shown_classes = HashSet::new();
        let filter_opacity = 250;
        let mask = generate_mask(&current_bbs, &shown_classes, size, filter_opacity);
        let mask_texture = cc
            .egui_ctx
            .load_texture("mask", mask, egui::TextureFilter::Linear);

        Box::new(Self {
            image_texture,
            mask_texture,
            bbox_input: BBoxInput::None,
            dataset: Dataset::from_input_dir().unwrap(),
            current_class: Card::V2,
            image_rect: Rect::NOTHING,
            filter: false,
            filter_opacity,
            shown_classes,
            current_label: current_bbs,
        })
    }
}

fn pos_inside_label_box(label: &YoloLabel<Card>, pos: Pos2, size: Vec2) -> bool {
    label.iter().any(|l| l.rect(size).contains(pos))
}
fn generate_mask(
    label: &YoloLabel<Card>,
    shown_classes: &HashSet<Card>,
    size: Vec2,
    opacity: u8,
) -> ColorImage {
    let highlighted_label = label
        .iter()
        .cloned()
        .filter(|bb| shown_classes.contains(&bb.class()))
        .collect();
    let width = size.x as usize;
    let height = size.y as usize;
    let mask = RgbaImage::from_fn(width as u32, height as u32, |x, y| {
        let pos = Pos2::new(x as f32, y as f32);
        if pos_inside_label_box(&highlighted_label, pos, size) {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([0, 0, 0, opacity])
        }
    });
    let pixels = mask.as_flat_samples();
    ColorImage::from_rgba_unmultiplied([width, height], pixels.as_slice())
}

impl Boundrs {
    fn to_img_coordinates(&self, pos: Pos2) -> Pos2 {
        (pos - self.image_rect.left_top()).to_pos2()
    }
    fn to_screen_coordinates(&self, pos: Pos2) -> Pos2 {
        pos + self.image_rect.left_top().to_vec2()
    }

    pub fn remove_labels(&mut self, pos: Pos2) {
        let size = self.image_texture.size_vec2();
        self.current_label
            .retain(|label| !label.rect(size).contains(pos));
    }
    pub fn add_bb(&mut self, bb: YoloBB<Card>) {
        self.current_label.push(bb)
    }

    pub fn repeat_bbs(&mut self) -> Result<()> {
        let yolo_label = self.dataset.previous_label()?;
        self.current_label = yolo_label;
        Ok(())
    }
    fn remove_bbs(&mut self, pos: Pos2) {
        self.remove_labels(pos);
    }

    fn handle_img_response(&mut self, img_response: Response, ui: &mut Ui) {
        if img_response.secondary_clicked() {
            let screen_pos = img_response.interact_pointer_pos().unwrap();
            let pos = self.to_img_coordinates(screen_pos);
            self.remove_bbs(pos);
            self.update_mask(ui.ctx());
        }

        // secondary click also regiesters a drag, therefore early return
        if ui.input().pointer.button_down(PointerButton::Secondary) {
            return;
        }
        self.bbox_input = match self.bbox_input {
            BBoxInput::None if img_response.drag_started() => {
                let screen_pos = img_response.interact_pointer_pos().unwrap();
                let pos = self.to_img_coordinates(screen_pos);
                BBoxInput::Partial(pos)
            }
            BBoxInput::None => BBoxInput::None,
            BBoxInput::Partial(pos1) if img_response.drag_released() => {
                let screen_pos = img_response.interact_pointer_pos().unwrap();
                let pos2 = self.to_img_coordinates(screen_pos);
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
                let label = YoloBB::from_rect(
                    Rect::from_two_pos(pos1, pos2),
                    self.image_texture.size_vec2(),
                    class,
                );
                println!("{:?}", label);
                self.add_bb(label);
                self.update_mask(ui.ctx());
                BBoxInput::None
            }
        };
    }
    fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: Card) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::none(),
            class.color(),
            Stroke::none(),
        );
        let _text_rect = painter.text(
            text_pos,
            Align2::LEFT_BOTTOM,
            class.to_name(),
            FontId::monospace(35.0),
            Color32::BLACK,
        );
    }
    fn draw_bbs(&self, ui: &mut Ui) {
        let painter = ui.painter();
        let size = self.image_rect.size();
        for bb in &self.current_label {
            let color = bb.class().color();
            // TODO improve
            let screen_rect = [
                self.to_screen_coordinates(bb.rect(size).left_top()),
                self.to_screen_coordinates(bb.rect(size).right_bottom()),
            ]
            .into();
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
            let screen_pos = self.to_screen_coordinates(pos);
            self.draw_guide(ui, screen_pos);
        }
    }

    fn update_texture(&mut self, ctx: &Context) {
        let image = self.dataset.current_image().unwrap();
        self.image_texture = ctx.load_texture("my-image", image, egui::TextureFilter::Linear);
    }
    fn update_mask(&mut self, ctx: &Context) {
        if self.filter {
            let mask = generate_mask(
                &self.current_label,
                &self.shown_classes,
                self.image_texture.size_vec2(),
                self.filter_opacity,
            );
            self.mask_texture = ctx.load_texture("mask", mask, egui::TextureFilter::Linear);
        }
    }

    fn classes_pressed(&self, ctx: &Context) -> HashSet<Card> {
        let mut classes = HashSet::new();
        for (keys, class) in Card::shortcuts() {
            if keys.iter().all(|key| ctx.input().key_pressed(*key)) {
                classes.insert(class);
            }
        }
        classes
    }

    fn handle_class_keys(&mut self, ctx: &Context) {
        let classes = self.classes_pressed(ctx);
        if self.filter {
            self.shown_classes = self
                .shown_classes
                .symmetric_difference(&classes)
                .copied()
                .collect();
            self.update_mask(ctx);
        } else if let Some(class) = classes.into_iter().next() {
            self.current_class = class;
        }
    }

    fn handle_left_right(&mut self, ctx: &Context) {
        let next_pressed =
            ctx.input().key_pressed(egui::Key::ArrowRight) | ctx.input().key_pressed(egui::Key::D);
        let previous_pressed =
            ctx.input().key_pressed(egui::Key::ArrowLeft) | ctx.input().key_pressed(egui::Key::A);

        let movement = match (next_pressed, previous_pressed, self.filter) {
            (true, false, false) => DatasetMovement::Next,
            (false, true, false) => DatasetMovement::Previous,
            (true, false, true) => DatasetMovement::NextContaining(self.shown_classes.clone()),
            (false, true, true) => DatasetMovement::PreviousContaining(self.shown_classes.clone()),
            _ => return,
        };
        self.dataset_move(movement, ctx);
    }

    fn dataset_move(&mut self, movement: DatasetMovement<Card>, ctx: &Context) {
        self.dataset
            .go(movement, self.current_label.clone())
            .unwrap();
        self.current_label = self.dataset.current_label().unwrap();
        self.update_texture(ctx);
        self.update_mask(ctx);
    }
}

impl eframe::App for Boundrs {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::Window::new("Boundrs Labeling").show(ctx, |ui| {
            let filename = self.dataset.current_name();
            ui.horizontal(|ui| {
                ui.label("Current image:");
                ui.label(filename);
            });
            ui.horizontal(|ui| {
                ui.label("Progress");
                let (_, current, max) = self.dataset.get_progress();
                ui.add(
                    ProgressBar::new(current as f32 / max as f32)
                        .show_percentage()
                        .text(format!("{current} out of {max} images")),
                );
            });
            ui.add(DragValue::from_get_set(|new_pos| {
                if let Some(new_pos) = new_pos {
                    self.dataset_move(DatasetMovement::JumpTo(new_pos as usize), ctx);
                }
                self.dataset.get_progress().1 as f64
            }));
            ui.horizontal(|ui| {
                ui.label("Filter opacity");
                ui.add(Slider::new(&mut self.filter_opacity, 0..=255));
            });
            ui.horizontal(|ui| {
                ui.label("Shown classes:");
                ui.label(format!("{:?}", self.shown_classes));
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                // Draw image
                let img_response = ui.add(
                    egui::Image::new(&self.image_texture, self.image_texture.size_vec2())
                        .sense(Sense::click_and_drag()),
                );
                self.image_rect = img_response.rect;

                // filter with mask
                if self.filter {
                    ui.put(
                        self.image_rect,
                        egui::Image::new(&self.mask_texture, self.mask_texture.size_vec2()),
                    );
                }

                // Draw guides
                let pos = ctx.input().pointer.hover_pos();
                if let Some(pos) = pos {
                    self.draw_guide(ui, pos)
                }
                self.draw_partial_box(ui);

                // Draw bbs
                self.draw_bbs(ui);

                // Handle clicks for bbs
                self.handle_img_response(img_response, ui);

                // Handle prev next picture keyboard
                self.handle_left_right(ctx);

                // Handle class setting
                self.handle_class_keys(ctx);

                // Handle filter mode
                let filter_pressed = ctx.input().key_pressed(egui::Key::F);
                if filter_pressed {
                    self.filter = !self.filter;
                }

                // Handle repeat button
                if ctx.input().key_pressed(egui::Key::R) {
                    self.repeat_bbs().unwrap();
                }
            });
    }
}
