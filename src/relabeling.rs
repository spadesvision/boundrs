use anyhow::Result;
use egui::*;
use log::error;

use crate::{
    dataset::{Datapoint, Dataset, DatasetMovement, DynLabel, DynLabelConfig, YoloBB, YoloLabel},
    Tool,
};
// use image::{Rgba, RgbaImage};

pub struct Relabeling {
    // index of currently editing label in old_label
    highlighted: Option<usize>,

    new_prefix: String,
    old_prefix: String,
    // image_texture: egui::TextureHandle,
    old_config: DynLabelConfig,
    new_config: DynLabelConfig,
    old_label: YoloLabel,
    new_label: YoloLabel,
    repeat_iou: f32,
    auto_repeat: bool,
}

impl Relabeling {
    pub fn new(
        old_prefix: &str,
        new_prefix: &str,
        old_config: DynLabelConfig,
        new_config: DynLabelConfig,
        initial: &Datapoint,
    ) -> Self {
        // let image = old_dataset.current_image().unwrap();
        // let image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
        let old_label = initial.load_label().unwrap();
        let new_label = initial
            .remove_prefix(old_prefix)
            .add_prefix(new_prefix)
            .load_label()
            .unwrap();
        let highlighted = None;
        Relabeling {
            new_prefix: new_prefix.to_string(),
            old_prefix: old_prefix.to_string(),
            highlighted,
            old_config,
            new_config,
            old_label,
            new_label,
            repeat_iou: 0.87,
            auto_repeat: true,
        }
        // TODO how could we handle this?
        // relabeling.highlighted = relabeling.find_next_highlighted();
    }
    // pub fn draw_img(&mut self, ui: &mut Ui) -> Response {
    //     let img_response = ui.add(
    //         egui::Image::new(
    //             &self.image_texture,
    //             self.image_texture.size_vec2() * self.zoom,
    //         )
    //         .sense(Sense::click_and_drag()),
    //     );
    //     self.img_rect = img_response.rect;
    //     img_response
    // }
    fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: DynLabel) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::ZERO,
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
    fn draw_bbs(&self, ui: &mut Ui, img_rect: Rect) {
        let painter = ui.painter();
        for bb in self.old_label.iter() {
            let color = bb.class(&self.old_config).color;
            let screen_rect = bb.to_screen_rect(img_rect);
            painter.rect_stroke(screen_rect, Rounding::ZERO, Stroke::new(2.0, color));
            let text_pos = screen_rect.left_bottom();
            self.draw_label_text(painter, text_pos, bb.class(&self.old_config));
        }
        for bb in self.new_label.iter() {
            let color = bb.class(&self.new_config).color;
            let screen_rect = bb.to_screen_rect(img_rect);
            painter.rect_stroke(screen_rect, Rounding::ZERO, Stroke::new(2.0, color));
            let text_pos = screen_rect.left_top();
            self.draw_label_text(painter, text_pos, bb.class(&self.new_config));
        }
    }
    fn find_next_highlighted(&mut self) -> Option<usize> {
        // let size = self.img_rect.size();
        let img_rect = Rect::from_min_size(Pos2::from([0.0, 0.0]), Vec2::from([1.0, 1.0]));
        self.old_label.sort_by(|a, b| a.x.total_cmp(&b.x));
        for (i, old_bbs) in self.old_label.iter().enumerate() {
            if self.new_label.iter().all(|new_bbs| {
                let old_rect = old_bbs.to_screen_rect(img_rect);
                let new_rect = new_bbs.to_screen_rect(img_rect);
                let iou = old_rect.intersect(new_rect).area() / old_rect.union(new_rect).area();
                iou < 0.95
            }) {
                return Some(i);
            }
        }
        None
    }
    fn draw_highlight(&self, ui: &mut Ui, img_rect: Rect) {
        if let Some(highlighted) = self.highlighted {
            let Some(bb) = &self.old_label.get(highlighted) else {
                error!(
                    "highlighted is out of range {highlighted} >= {}",
                    &self.old_label.len()
                );
                return;
            };
            let screen_rect = bb.to_screen_rect(img_rect);
            ui.painter().rect_stroke(
                screen_rect,
                Rounding::ZERO,
                Stroke::new(8.0, Color32::WHITE),
            );
        }
    }
    // pub fn update_texture(&mut self, ctx: &Context) {
    //     let image = self.old_dataset.current_image().unwrap();
    // self.image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
    // }
    // pub fn go(&mut self, movement: DatasetMovement, _ctx: &Context) {
    //     // TODO actually check if labels match correctly, not only length
    //     if movement == DatasetMovement::Next && self.new_label.len() != self.old_label.len() {
    //         println!(
    //             "Missing labels: len new {} vs len old {}",
    //             self.new_label.len(),
    //             self.old_label.len(),
    //         );
    //         return;
    //     }
    //     self.old_dataset
    //         .go(movement.clone(), self.old_label.clone(), false)
    //         .unwrap();
    //     self.new_dataset
    //         .go(movement, self.new_label.clone(), true)
    //         .unwrap();
    //     self.old_label = self.old_dataset.current_label().unwrap();
    //     self.new_label = self.new_dataset.current_label().unwrap();
    //     self.highlighted = self.find_next_highlighted();
    //     // self.update_texture(ctx);
    // }
    fn handle_left_right(&mut self, ui: &Ui, dataset: &mut Dataset, img_rect: Rect) {
        let next_pressed = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
        let previous_pressed = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));

        let movement = match (next_pressed, previous_pressed) {
            (true, false) => DatasetMovement::Next,
            (false, true) => DatasetMovement::Previous,
            _ => return,
        };

        println!("trying to move dataset {movement:?}");

        if movement == DatasetMovement::Next && self.new_label.is_empty() {
            self.save_state(dataset.current()).unwrap();
            dataset.go(movement, None).unwrap();
            self.refresh_state(dataset.current()).unwrap();
            self.repeat_bbs(dataset, img_rect).unwrap();
            return;
        }
        self.save_state(dataset.current()).unwrap();
        dataset.go(movement, None).unwrap();
        self.refresh_state(dataset.current()).unwrap();
    }
    fn handle_clear(&mut self, ctx: &Context, img_rect: Rect) {
        let delete_pressed = ctx.input(|i| i.key_pressed(egui::Key::Delete));
        if delete_pressed {
            self.new_label = vec![];
        }
        self.highlighted = self.find_next_highlighted();
    }
    fn new_class_pressed(&self, ui: &Ui) -> Option<usize> {
        // TODO fix this
        ui.input(|i| {
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

    pub fn handle_class_keys(&mut self, ui: &Ui, img_rect: Rect, dataset: &mut Dataset) {
        if let (Some(suit_usize), Some(highlighted)) =
            (self.new_class_pressed(ui), self.highlighted)
        {
            let old_bbx = self.old_label[highlighted];
            // let class = old_bbx.class(&self.old_config);
            // let card = Card::from_usize(class.0);
            let new_class = self
                .new_config
                .label_from_usize(suit_usize * 13 + old_bbx.class(&self.old_config).i % 13)
                .expect("Remap issue, usize to high");
            // let new_class = CardSuit(card, suit);
            let new_bbs = YoloBB::from_rect(old_bbx.to_screen_rect(img_rect), img_rect, &new_class);
            self.new_label.push(new_bbs);
            self.highlighted = self.find_next_highlighted();
            while self.new_label.len() == self.old_label.len()
                && self.highlighted.is_none()
                && self.auto_repeat
            {
                self.save_state(dataset.current()).unwrap();
                dataset.go(DatasetMovement::Next, None).unwrap();
                self.repeat_bbs(dataset, img_rect).unwrap();
            }
        }
    }
    pub fn take_similar_bbs(&mut self, new_label_candidates: Vec<YoloLabel>, img_rect: Rect) {
        self.new_label = vec![];
        for old_bbs in self.old_label.iter() {
            for new_bbs in new_label_candidates.iter().flatten() {
                let old_rect = old_bbs.to_screen_rect(img_rect);
                let new_rect = new_bbs.to_screen_rect(img_rect);
                let intersect = old_rect.intersect(new_rect).area();
                let union = old_rect.union(new_rect).area();
                let iou = intersect / union;
                // TODO generalize, probably from config
                if iou > self.repeat_iou && old_bbs.class_num == new_bbs.class_num % 13 {
                    let new_label =
                        YoloBB::from_rect(old_rect, img_rect, &new_bbs.class(&self.new_config));
                    self.new_label.push(new_label);
                    break;
                }
            }
        }
    }
    pub fn repeat_bbs(&mut self, dataset: &Dataset, img_rect: Rect) -> Result<()> {
        let previous_labels = dataset.previous_labels(2)?;
        self.take_similar_bbs(previous_labels, img_rect);
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
    pub fn handle_img_response(&mut self, img_response: &Response) {
        if img_response.secondary_clicked() {
            println!("secondary clicked");
            let screen_pos = img_response.interact_pointer_pos().unwrap();
            self.remove_bbs(screen_pos, img_response.rect);
        }
    }
}

impl Tool for Relabeling {
    fn draw_in_central_panel(
        &mut self,
        central_panel: &mut Ui,
        img_response: Response,
        dataset: &mut Dataset,
    ) -> Result<()> {
        // Draw bbs
        self.draw_bbs(central_panel, img_response.rect);
        self.draw_highlight(central_panel, img_response.rect);

        if img_response.has_focus() {
            // Handle repeat button
            if central_panel.ctx().input(|i| i.key_pressed(egui::Key::R)) {
                self.repeat_bbs(dataset, img_response.rect).unwrap();
            }
            // Handle class setting
            self.handle_class_keys(central_panel, img_response.rect, dataset);

            // Handle labels clearing
            self.handle_clear(central_panel.ctx(), img_response.rect);

            // Handle right click
            self.handle_img_response(&img_response);

            // Handle left and right clicks
            self.handle_left_right(central_panel, dataset, img_response.rect)
        }
        Ok(())
        // }
    }

    fn draw_ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        ui.horizontal(|ui| {
            ui.label("Repeat iou:");
            ui.add(DragValue::new(&mut self.repeat_iou).speed(0.01));
            ui.checkbox(&mut self.auto_repeat, "Auto repeat")
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
        Ok(())
    }

    fn name(&self) -> &str {
        "Relabeling"
    }

    fn refresh_state(&mut self, datapoint: &Datapoint) -> anyhow::Result<()> {
        self.old_label = datapoint.load_label()?;
        self.new_label = datapoint
            .remove_prefix(&self.old_prefix)
            .add_prefix(&self.new_prefix)
            .load_label()
            .unwrap();
        self.highlighted = self.find_next_highlighted();
        Ok(())
    }

    fn save_state(&self, datapoint: &Datapoint) -> anyhow::Result<()> {
        let new_datapoint = datapoint
            .remove_prefix(&self.old_prefix)
            .add_prefix(&self.new_prefix);
        new_datapoint.save_label(self.new_label.clone())
    }

    // pub fn prepare_switch(&mut self) -> SyncDatasets {
    //     let (_, current_pos, _) = self.old_dataset.get_progress();
    //     let (_, new_pos, _) = self.old_dataset.get_progress();
    //     assert_eq!(current_pos, new_pos);
    //     let movement = DatasetMovement::JumpTo(current_pos);
    //     // We only save the new_dataset stuff, not the old stuff, that should only be managed by he labeling application
    //     self.old_dataset.go(movement.clone(), None).unwrap();
    //     self.new_dataset
    //         .go(movement, Some(self.new_label.clone()))
    //         .unwrap();
    //     SyncDatasets { current_pos }
    // }
    // pub fn refresh_after_switch(&mut self, sync: &SyncDatasets, _ctx: &Context) {
    //     let SyncDatasets { current_pos } = sync;
    //     // TODO fix this
    //     let movement = DatasetMovement::JumpTo(*current_pos);
    //     self.old_dataset.go(movement.clone(), None).unwrap();
    //     self.new_dataset.go(movement, None).unwrap();
    //     self.old_label = self.old_dataset.current_label().unwrap();
    //     self.new_label = self.new_dataset.current_label().unwrap();
    //     self.highlighted = self.find_next_highlighted();
    //     // self.update_texture(ctx);
    // }
}
