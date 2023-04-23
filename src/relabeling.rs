use std::path::Path;

use anyhow::Result;
use eframe::egui;
use egui::*;

use crate::dataset::{Dataset, DatasetMovement, DynLabel, DynLabelConfig, YoloBB, YoloLabel};
// use image::{Rgba, RgbaImage};

pub struct Relabeling {
    // index of currently editing label in old_label
    pub highlighted: Option<usize>,
    zoom: f32,
    image_texture: egui::TextureHandle,
    img_rect: Rect,
    pub old_dataset: Dataset,
    old_config: DynLabelConfig,
    new_dataset: Dataset,
    new_config: DynLabelConfig,
    pub old_label: YoloLabel,
    new_label: YoloLabel,
    repeat_iou: f32,
}

impl Relabeling {
    pub fn new(
        ctx: &Context,
        dataset_dir: &Path,
        old_prefix: &str,
        new_prefix: &str,
        old_config: DynLabelConfig,
        new_config: DynLabelConfig,
    ) -> Self {
        let old_dataset = Dataset::with_prefix(dataset_dir, old_prefix).unwrap();
        let new_dataset = Dataset::with_prefix(dataset_dir, new_prefix).unwrap();
        let image = old_dataset.current_image().unwrap();
        let image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
        let old_label = old_dataset.current_label().unwrap();
        let new_label = new_dataset.current_label().unwrap();
        let highlighted = None;
        let mut relabeling = Relabeling {
            highlighted,
            zoom: 1.8,
            image_texture,
            img_rect: Rect::NOTHING,
            old_dataset,
            old_config,
            old_label,
            new_dataset,
            new_config,
            new_label,
            repeat_iou: 0.87,
        };
        relabeling.highlighted = relabeling.find_next_highlighted();
        relabeling
    }
    pub fn draw_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        let filename = self.old_dataset.current_name();
        ui.horizontal(|ui| {
            ui.label("Current image:");
            ui.label(filename);
        });
        ui.horizontal(|ui| {
            ui.label("Progress");
            ui.add(DragValue::from_get_set(|new_pos| {
                if let Some(new_pos) = new_pos {
                    self.go(DatasetMovement::JumpTo(new_pos as usize), ctx);
                }
                self.old_dataset.get_progress().1 as f64
            }));
            let (_, current, max) = self.old_dataset.get_progress();
            ui.add(
                ProgressBar::new(current as f32 / max as f32)
                    .show_percentage()
                    .text(format!("{current} out of {max} images")),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Zoom image:");
            ui.add(DragValue::new(&mut self.zoom).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Repeat iou:");
            ui.add(DragValue::new(&mut self.repeat_iou).speed(0.01));
        });
        ui.vertical(|ui| {
            ui.separator();
            ui.label(RichText::new("How to use").heading());
            ui.label(RichText::new(
                "The highlighted box is ready to be remapped with these keybindings:",
            ));
            ui.label(RichText::new("H: Heart"));
            ui.label(RichText::new("D: Diamond"));
            ui.label(RichText::new("C: Clubs"));
            ui.label(RichText::new("S: Spades"));
            ui.label(RichText::new("Clear (mapped) labels: Delete"));
            ui.label(RichText::new("Repeat previous labels: R"));
            ui.label(RichText::new("Go left or right: Left Arrow or Right Arrow"));
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
    pub fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: DynLabel) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::none(),
            class.color,
            Stroke::NONE,
        );
        let _text_rect = painter.text(
            text_pos,
            Align2::LEFT_BOTTOM,
            class.name,
            FontId::monospace(35.0),
            Color32::BLACK,
        );
    }
    pub fn draw_bbs(&self, ui: &mut Ui) {
        let painter = ui.painter();
        for bb in self.old_label.iter() {
            let color = bb.class(&self.old_config).color;
            let screen_rect = bb.to_screen_rect(self.img_rect);
            painter.rect_stroke(screen_rect, Rounding::none(), Stroke::new(2.0, color));
            let text_pos = screen_rect.left_bottom();
            self.draw_label_text(painter, text_pos, bb.class(&self.old_config));
        }
        for bb in self.new_label.iter() {
            let color = bb.class(&self.new_config).color;
            let screen_rect = bb.to_screen_rect(self.img_rect);
            painter.rect_stroke(screen_rect, Rounding::none(), Stroke::new(2.0, color));
            let text_pos = screen_rect.left_top();
            self.draw_label_text(painter, text_pos, bb.class(&self.new_config));
        }
    }
    pub fn find_next_highlighted(&self) -> Option<usize> {
        // let size = self.img_rect.size();
        for (i, old_bbs) in self.old_label.iter().enumerate() {
            if self.new_label.iter().all(|new_bbs| {
                let old_rect = old_bbs.to_screen_rect(self.img_rect);
                let new_rect = new_bbs.to_screen_rect(self.img_rect);
                let iou = old_rect.intersect(new_rect).area() / old_rect.union(new_rect).area();
                iou < 0.95
            }) {
                return Some(i);
            }
        }
        None
    }
    pub fn draw_highlight(&self, ui: &mut Ui) {
        if let Some(highlighted) = self.highlighted {
            let bb = &self.old_label[highlighted];
            let screen_rect = bb.to_screen_rect(self.img_rect);
            ui.painter().rect_stroke(
                screen_rect,
                Rounding::none(),
                Stroke::new(8.0, Color32::WHITE),
            );
        }
    }
    pub fn update_texture(&mut self, ctx: &Context) {
        let image = self.old_dataset.current_image().unwrap();
        self.image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
    }
    pub fn go(&mut self, movement: DatasetMovement, ctx: &Context) {
        // TODO actually check if labels match correctly, not only length
        if movement == DatasetMovement::Next && self.new_label.len() != self.old_label.len() {
            println!(
                "Missing labels: len new {} vs len old {}",
                self.new_label.len(),
                self.old_label.len(),
            );
            return;
        }
        self.old_dataset
            .go(movement.clone(), self.old_label.clone())
            .unwrap();
        self.new_dataset
            .go(movement, self.new_label.clone())
            .unwrap();
        self.old_label = self.old_dataset.current_label().unwrap();
        self.new_label = self.new_dataset.current_label().unwrap();
        self.highlighted = self.find_next_highlighted();
        self.update_texture(ctx);
    }
    pub fn handle_left_right(&mut self, ctx: &Context) {
        let next_pressed = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
        let previous_pressed = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));

        let movement = match (next_pressed, previous_pressed) {
            (true, false) => DatasetMovement::Next,
            (false, true) => DatasetMovement::Previous,
            _ => return,
        };

        // TODO fix this clone... weird code
        self.go(movement.clone(), ctx);
        if movement == DatasetMovement::Next && self.new_label.is_empty() {
            self.repeat_bbs().unwrap();
        }
    }
    pub fn handle_clear(&mut self, ctx: &Context) {
        let delete_pressed = ctx.input(|i| i.key_pressed(egui::Key::Delete));
        if delete_pressed {
            self.new_label = vec![];
        }
        self.highlighted = self.find_next_highlighted();
    }
    fn new_class_pressed(&self, ctx: &Context) -> Option<usize> {
        // TODO fix this
        ctx.input(|i| {
            if i.key_pressed(Key::H) {
                Some(0)
            } else if i.key_pressed(Key::D) {
                Some(1)
            } else if i.key_pressed(Key::C) {
                Some(2)
            } else if i.key_pressed(Key::S) {
                Some(3)
            } else {
                None
            }
        })
    }

    pub fn handle_class_keys(&mut self, ctx: &Context) {
        if let (Some(suit_usize), Some(highlighted)) =
            (self.new_class_pressed(ctx), self.highlighted)
        {
            let old_bbx = self.old_label[highlighted];
            // let class = old_bbx.class(&self.old_config);
            // let card = Card::from_usize(class.0);
            let new_class = self
                .new_config
                .label_from_usize(suit_usize * 13 + old_bbx.class(&self.old_config).i)
                .expect("Remap issue, usize to high");
            // let new_class = CardSuit(card, suit);
            let new_bbs = YoloBB::from_rect(
                old_bbx.to_screen_rect(self.img_rect),
                self.img_rect,
                &new_class,
            );
            self.new_label.push(new_bbs);
            self.highlighted = self.find_next_highlighted();
            while self.new_label.len() == self.old_label.len() && self.highlighted.is_none() {
                self.go(DatasetMovement::Next, ctx);
                self.repeat_bbs().unwrap();
            }
        }
    }
    pub fn take_similar_bbs(&mut self, new_label_candidates: Vec<YoloLabel>) {
        self.new_label = vec![];
        for old_bbs in self.old_label.iter() {
            for new_bbs in new_label_candidates.iter().flatten() {
                let old_rect = old_bbs.to_screen_rect(self.img_rect);
                let new_rect = new_bbs.to_screen_rect(self.img_rect);
                let intersect = old_rect.intersect(new_rect).area();
                let union = old_rect.union(new_rect).area();
                let iou = intersect / union;
                // TODO generalize, probably from config
                if iou > self.repeat_iou && old_bbs.class_num == new_bbs.class_num % 13 {
                    let new_label = YoloBB::from_rect(
                        old_rect,
                        self.img_rect,
                        &new_bbs.class(&self.new_config),
                    );
                    self.new_label.push(new_label);
                    break;
                }
            }
        }
    }
    pub fn repeat_bbs(&mut self) -> Result<()> {
        let previous_labels = self.new_dataset.previous_labels(2)?;
        self.take_similar_bbs(previous_labels);
        self.highlighted = self.find_next_highlighted();
        Ok(())
    }
    pub fn remove_labels(&mut self, pos: Pos2, img_rect: Rect) {
        // self.old_label
        //     .retain(|label| !label.to_screen_rect(img_rect).contains(pos));
        self.new_label
            .retain(|label| !label.to_screen_rect(img_rect).contains(pos));
        self.highlighted = self.find_next_highlighted();
    }
    fn remove_bbs(&mut self, pos: Pos2, img_rect: Rect) {
        self.remove_labels(pos, img_rect);
    }
    pub fn handle_img_response(&mut self, img_response: Response, _ui: &mut Ui) {
        if img_response.secondary_clicked() {
            println!("secondary clicked");
            let screen_pos = img_response.interact_pointer_pos().unwrap();
            self.remove_bbs(screen_pos, img_response.rect);
        }
    }
}
