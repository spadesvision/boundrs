use anyhow::Result;
use egui::{Key, *};
use std::{collections::HashSet, path::Path};

use crate::{
    dataset::{Datapoint, Dataset, DatasetMovement, DynLabel, DynLabelConfig, YoloBB, YoloLabel},
    SyncDatasets, Tool,
};
use image::{Rgba, RgbaImage};

#[derive(Debug, Clone, Copy)]
enum BBoxInput {
    None,
    Partial(Pos2),
    Finished(Pos2, Pos2),
}

pub struct Labeling {
    label_config: DynLabelConfig,
    // zoom: f32,
    // TODO move this to main app, pass new texture out of label / relabel function
    current_label: YoloLabel,
    mask_texture: Option<egui::TextureHandle>,
    bbox_input: BBoxInput,
    repeat_mode: bool,
    current_class: DynLabel,
    filter: bool,
    filter_opacity: u8,
    shown_classes: HashSet<usize>,
    last_time: f64,
}

impl Labeling {
    pub fn new(
        cc: &Context,
        dataset_dir: &Path,
        prefix: &str,
        label_config: DynLabelConfig,
        initial: &Datapoint,
    ) -> Self {
        let current_class = label_config
            .label_from_usize(0)
            .expect("At least 1 label is needed");
        let current_label = initial.load_label().unwrap();
        Labeling {
            label_config,
            current_label,
            mask_texture: None,
            bbox_input: BBoxInput::None,
            repeat_mode: false,
            current_class,
            filter: false,
            filter_opacity: 250,
            shown_classes: HashSet::new(),
            last_time: 0.0,
        }
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
    // fn update_texture(&mut self, ctx: &Context) {
    // let image = self.dataset.current_image().unwrap();
    // self.image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
    // }
    fn update_mask(&mut self, ctx: &Context, img_rect: Rect) {
        if self.filter {
            let mask = generate_mask(
                &self.current_label,
                &self.shown_classes,
                img_rect,
                self.filter_opacity,
            );
            self.mask_texture = Some(ctx.load_texture("mask", mask, egui::TextureOptions::LINEAR));
        }
    }
    pub fn remove_labels(&mut self, pos: Pos2, img_rect: Rect) {
        self.current_label
            .retain(|label| !label.to_screen_rect(img_rect).contains(pos));
    }
    pub fn add_bb(&mut self, bb: YoloBB) {
        self.current_label.push(bb)
    }

    pub fn repeat_bbs_inside(
        &mut self,
        rect: Rect,
        img_rect: Rect,
        dataset: &Dataset,
    ) -> Result<()> {
        // get two coordinates to repeat only the labels completely in this box
        let prev_label = dataset.previous_labels(1)?[0].clone();
        let prev_label_inside = prev_label
            .into_iter()
            .filter(|l| rect.contains_rect(l.to_screen_rect(img_rect)));
        self.current_label = self
            .current_label
            .clone()
            .into_iter()
            .filter(|l| !rect.contains_rect(l.to_screen_rect(img_rect)))
            .chain(prev_label_inside)
            .collect();
        Ok(())
    }
    fn remove_bbs(&mut self, pos: Pos2, img_rect: Rect) {
        self.remove_labels(pos, img_rect);
    }
    fn draw_bbs(&self, ui: &mut Ui, img_rect: Rect) {
        // let img_rect = self.img_rect;
        let painter = ui.painter();
        // let size = self.img_rect.size();
        for bb in &self.current_label {
            let color = bb.class(&self.label_config).color;
            let screen_rect = bb.to_screen_rect(img_rect);
            painter.rect_stroke(screen_rect, Rounding::ZERO, Stroke::new(2.0, color));
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
            Rounding::ZERO,
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
    fn handle_img_response(&mut self, img_response: Response, ui: &mut Ui, dataset: &Dataset) {
        if img_response.secondary_clicked() {
            let screen_pos = img_response.interact_pointer_pos().unwrap();
            self.remove_bbs(screen_pos, img_response.rect);
            self.update_mask(ui.ctx(), img_response.rect);
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
                self.update_mask(ui.ctx(), img_response.rect);
                BBoxInput::None
            }
        };
        if let BBoxInput::Finished(pos1, pos2) = self.bbox_input {
            if self.repeat_mode {
                let rect = Rect::from_two_pos(pos1, pos2);
                self.repeat_bbs_inside(rect, img_response.rect, dataset)
                    .unwrap();
                self.repeat_mode = false;
                self.bbox_input = BBoxInput::None
            } else {
                let img_rect = img_response.rect;
                let label_rect = Rect::from_two_pos(pos1, pos2);
                let label = YoloBB::from_rect(label_rect, img_rect, &self.current_class);
                println!("{label:?}");
                self.add_bb(label);
                self.update_mask(ui.ctx(), img_response.rect);
                self.bbox_input = BBoxInput::None
            }
        }
    }
    fn class_pressed(&self, ui: &Ui) -> Option<DynLabel> {
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
        if ui.input(|i| i.time - self.last_time > 0.3) {
            return ui.input(|i| self.label_config.label_from_keys(&i.keys_down));
        }
        None
    }

    fn handle_class_keys(&mut self, ui: &Ui, img_rect: Rect) {
        if let Some(class) = self.class_pressed(ui) {
            self.last_time = ui.input(|i| i.time);
            if self.filter {
                if self.shown_classes.contains(&class.i) {
                    self.shown_classes.remove(&class.i);
                } else {
                    self.shown_classes.insert(class.i);
                }
                self.update_mask(ui.ctx(), img_rect);
            } else {
                self.current_class = class.clone();
            }
        }
    }
    fn handle_left_right(&mut self, ui: &Ui, ctx: &Context) {
        let next_pressed = ui.input(|i| i.key_pressed(Key::ArrowRight));
        let previous_pressed = ui.input(|i| i.key_pressed(Key::ArrowLeft));

        let movement = match (next_pressed, previous_pressed, self.filter) {
            (true, false, false) => DatasetMovement::Next,
            (false, true, false) => DatasetMovement::Previous,
            (true, false, true) => DatasetMovement::NextContaining(self.shown_classes.clone()),
            (false, true, true) => DatasetMovement::PreviousContaining(self.shown_classes.clone()),
            _ => return,
        };
        // self.dataset_move(movement);
    }

    // pub fn dataset_move(&mut self, movement: DatasetMovement) {
    //     self.dataset
    //         .go(movement, self.current_label.clone(), true)
    //         .unwrap();
    //     self.current_label = self.dataset.current_label().unwrap();
    //     // self.update_texture(ctx);
    // self.update_mask(ctx);
    // }
    pub fn draw_central_panel(&mut self, ui: &mut Ui) {}

    // pub fn prepare_switch(&mut self) -> SyncDatasets {
    //     let (_, current_pos, _) = self.dataset.get_progress();
    //     self.dataset
    //         .go(
    //             DatasetMovement::JumpTo(current_pos),
    //             self.current_label.clone(),
    //             true,
    //         )
    //         .unwrap();
    //     SyncDatasets { current_pos }
    // }
    // pub fn refresh_after_switch(&mut self, sync: &SyncDatasets, ctx: &Context) {
    //     let SyncDatasets { current_pos } = sync;
    //     let movement = DatasetMovement::JumpTo(*current_pos);
    //     self.dataset
    //         .go(movement, self.current_label.clone(), false)
    //         .unwrap();
    //     self.current_label = self.dataset.current_label().unwrap();
    //     // self.update_texture(ctx);
    //     // self.update_mask(ctx);
    // }
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

impl Tool for Labeling {
    fn draw_ui(&mut self, ui: &mut Ui) -> Result<()> {
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
        // ui.horizontal(|ui| {
        //     ui.label("Zoom image:");
        //     ui.add(DragValue::new(&mut self.zoom).speed(0.01));
        // });
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
        Ok(())
    }
    fn draw_in_central_panel(
        &mut self,
        central_panel: &mut Ui,
        img_response: Response,
        dataset: &Dataset,
    ) -> Result<()> {
        // let img_response = self.draw_img(ui);

        // filter
        // if self.filter {
        //     ui.put(
        //         self.img_rect,
        //         egui::Image::new(&self.mask_texture, self.mask_texture.size_vec2()),
        //     );
        // }

        // Draw guides
        let pos = central_panel.input(|i| i.pointer.hover_pos());
        if let Some(pos) = pos {
            self.draw_guide(central_panel, pos)
        }
        self.draw_partial_box(central_panel);

        // Draw bbs
        self.draw_bbs(central_panel, img_response.rect);

        // if img_response.clicked() || request_focus {
        //     img_response.request_focus();
        // }

        // if img_response.has_focus() {
        // Handle prev next picture keyboard
        // self.handle_left_right(central_panel, ctx);
        // Handle class setting
        self.handle_class_keys(central_panel, img_response.rect);

        // Handle clicks for bbs
        self.handle_img_response(img_response, central_panel, dataset);
        // Handle filter mode
        let filter_pressed = central_panel.input(|i| i.key_pressed(Key::F));
        if filter_pressed {
            self.filter = !self.filter;
        }

        // Handle repeat button
        if central_panel.input(|i| i.key_pressed(Key::R)) {
            self.repeat_mode = !self.repeat_mode;
        }
        // }
        Ok(())
    }
    fn refresh_state(&mut self, datapoint: &Datapoint) -> Result<()> {
        self.current_label = datapoint.load_label()?;
        Ok(())
    }
    fn save_state(&self, datapoint: &Datapoint) -> Result<()> {
        datapoint.save_label(self.current_label.clone())
    }

    fn name(&self) -> &str {
        "Labeling Tool"
    }
}
