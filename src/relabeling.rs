use anyhow::Result;
use eframe::egui;
use egui::*;
use std::collections::HashSet;

use crate::dataset::{
    BoundingBox, Card, CardSuit, Dataset, DatasetMovement, Label, Suit, YoloLabel,
};
// use image::{Rgba, RgbaImage};

pub struct Relabeling {
    // index of currently editing label in old_label
    highlighted: Option<usize>,
    image_texture: egui::TextureHandle,
    image_rect: Rect,
    old_dataset: Dataset<Card>,
    new_dataset: Dataset<CardSuit>,
    old_label: YoloLabel<Card>,
    new_label: YoloLabel<CardSuit>,
}

impl Relabeling {
    pub fn build_app(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
        let old_dataset = Dataset::from_input_dir().unwrap();
        let new_dataset = Dataset::with_label_prefix("new_").unwrap();
        let image = old_dataset.current_image().unwrap();
        let image_texture =
            cc.egui_ctx
                .load_texture("my-image", image, egui::TextureFilter::Linear);
        let old_label = old_dataset.current_label().unwrap();
        let new_label = new_dataset.current_label().unwrap();
        let highlighted = None;
        let mut relabeling = Relabeling {
            highlighted,
            image_texture,
            image_rect: Rect::NOTHING,
            old_dataset,
            new_dataset,
            old_label,
            new_label,
        };
        relabeling.highlighted = relabeling.find_next_highlighted();
        Box::new(relabeling)
    }
    fn to_screen_coordinates(&self, pos: Pos2) -> Pos2 {
        pos + self.image_rect.left_top().to_vec2()
    }
    fn draw_label_text<L: Label>(&self, painter: &Painter, text_pos: Pos2, class: L) {
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
        for bb in self.old_label.iter() {
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
        for bb in self.new_label.iter() {
            let color = bb.class().color();
            // TODO improve
            let screen_rect = [
                self.to_screen_coordinates(bb.rect(size).left_top()),
                self.to_screen_coordinates(bb.rect(size).right_bottom()),
            ]
            .into();
            painter.rect_stroke(screen_rect, Rounding::none(), Stroke::new(2.0, color));
            let text_pos = screen_rect.left_top();
            self.draw_label_text(painter, text_pos, bb.class());
        }
    }
    fn find_next_highlighted(&self) -> Option<usize> {
        let size = self.image_rect.size();
        for (i, old_bbs) in self.old_label.iter().enumerate() {
            if self.new_label.iter().all(|new_bbs| {
                let old_rect = old_bbs.rect(size);
                let new_rect = new_bbs.rect(size);
                let iou = old_rect.intersect(new_rect).area() / old_rect.union(new_rect).area();
                iou < 0.95
            }) {
                return Some(i);
            }
        }
        None
    }
    fn draw_highlight(&self, ui: &mut Ui) {
        if let Some(highlighted) = self.highlighted {
            let bb = &self.old_label[highlighted];
            let size = self.image_rect.size();
            let screen_rect = [
                self.to_screen_coordinates(bb.rect(size).left_top()),
                self.to_screen_coordinates(bb.rect(size).right_bottom()),
            ]
            .into();
            ui.painter().rect_stroke(
                screen_rect,
                Rounding::none(),
                Stroke::new(8.0, Color32::WHITE),
            );
        }
    }
    fn update_texture(&mut self, ctx: &Context) {
        let image = self.old_dataset.current_image().unwrap();
        self.image_texture = ctx.load_texture("my-image", image, egui::TextureFilter::Linear);
    }
    fn go(
        &mut self,
        move_old: DatasetMovement<Card>,
        move_new: DatasetMovement<CardSuit>,
        ctx: &Context,
    ) {
        if move_old == DatasetMovement::Next && self.new_label.len() != self.old_label.len() {
            println!(
                "Missing labels: len new {} vs len old {}",
                self.new_label.len(),
                self.old_label.len(),
            );
            return;
        }
        self.old_dataset
            .go(move_old, self.old_label.clone())
            .unwrap();
        self.new_dataset
            .go(move_new, self.new_label.clone())
            .unwrap();
        self.old_label = self.old_dataset.current_label().unwrap();
        self.new_label = self.new_dataset.current_label().unwrap();
        self.highlighted = self.find_next_highlighted();
        self.update_texture(ctx);
    }
    fn handle_left_right(&mut self, ctx: &Context) {
        let next_pressed = ctx.input().key_pressed(egui::Key::ArrowRight);
        let previous_pressed = ctx.input().key_pressed(egui::Key::ArrowLeft);

        let (move_old, move_new) = match (next_pressed, previous_pressed) {
            (true, false) => (DatasetMovement::Next, DatasetMovement::Next),
            (false, true) => (DatasetMovement::Previous, DatasetMovement::Previous),
            _ => return,
        };

        // TODO this is not good code...
        self.go(move_old.clone(), move_new, ctx);
        if move_old == DatasetMovement::Next && self.new_label.is_empty() {
            self.repeat_bbs().unwrap();
        }
    }
    fn handle_clear(&mut self, ctx: &Context) {
        let delete_pressed = ctx.input().key_pressed(egui::Key::Delete);
        if delete_pressed {
            self.new_label = vec![];
        }
        self.highlighted = self.find_next_highlighted();
    }
    fn classes_pressed(&self, ctx: &Context) -> HashSet<Suit> {
        let mut classes = HashSet::new();
        for (keys, class) in Suit::shortcuts() {
            if keys.iter().all(|key| ctx.input().key_pressed(*key)) {
                classes.insert(class);
            }
        }
        classes
    }
    fn handle_class_keys(&mut self, ctx: &Context) {
        let suits = self.classes_pressed(ctx);
        if let (Some(suit), Some(highlighted)) = (suits.into_iter().next(), self.highlighted) {
            let size = self.image_rect.size();
            let old_bbx = self.old_label[highlighted];
            let card = old_bbx.class();
            let new_class = CardSuit(card, suit);
            let new_bbs = BoundingBox::from_rect(old_bbx.rect(size), size, new_class);
            self.new_label.push(new_bbs);
            self.highlighted = self.find_next_highlighted();
            if self.new_label.len() == self.old_label.len() && self.highlighted.is_none() {
                let (move_old, move_new) = (DatasetMovement::Next, DatasetMovement::Next);
                self.go(move_old, move_new, ctx);
                self.repeat_bbs().unwrap();
            }
        }
    }
    fn take_similar_bbs(&mut self, new_label_candidate: YoloLabel<CardSuit>) {
        let size = self.image_rect.size();
        self.new_label = vec![];
        for old_bbs in self.old_label.iter() {
            for new_bbs in new_label_candidate.iter() {
                let old_rect = old_bbs.rect(size);
                let new_rect = new_bbs.rect(size);
                let intersect = old_rect.intersect(new_rect).area();
                let union = old_rect.union(new_rect).area();
                let iou = intersect / union;
                if iou > 0.95 {
                    let new_label = BoundingBox::from_rect(old_rect, size, new_bbs.class());
                    self.new_label.push(new_label)
                }
            }
        }
    }
    pub fn repeat_bbs(&mut self) -> Result<()> {
        let previous_label = self.new_dataset.previous_label()?;
        self.take_similar_bbs(previous_label);
        self.highlighted = self.find_next_highlighted();
        Ok(())
    }
}

impl eframe::App for Relabeling {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::Window::new("Boundrs Labeling").show(ctx, |ui| {
            let filename = self.old_dataset.current_name();
            ui.horizontal(|ui| {
                ui.label("Current image:");
                ui.label(filename);
            });
            ui.horizontal(|ui| {
                ui.label("Progress");
                let (_, current, max) = self.old_dataset.get_progress();
                ui.add(
                    ProgressBar::new(current as f32 / max as f32)
                        .show_percentage()
                        .text(format!("{current} out of {max} images")),
                );
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

                // Draw guides
                // let pos = ctx.input().pointer.hover_pos();
                // if let Some(pos) = pos {
                //     self.draw_guide(ui, pos)
                // }
                // self.draw_partial_box(ui);

                // Draw bbs
                self.draw_bbs(ui);
                self.draw_highlight(ui);

                // // Handle clicks for bbs
                // self.handle_img_response(img_response, ui);

                // Handle prev next picture keyboard
                self.handle_left_right(ctx);

                // Handle class setting
                self.handle_class_keys(ctx);

                // Handle labels clearing
                self.handle_clear(ctx);

                // Handle filter mode
                // let filter_pressed = ctx.input().key_pressed(egui::Key::F);
                // if filter_pressed {
                //     self.filter = !self.filter;
                // }

                // Handle repeat button
                if ctx.input().key_pressed(egui::Key::R) {
                    self.repeat_bbs().unwrap();
                }
            });
    }
}
