use anyhow::Result;
use bje_detections::Detections;
use egui::{Key, *};
use std::collections::HashSet;

use crate::{
    dataset::{BoundrsDetection, Dataset, DatasetMovement, DynLabel, DynLabelConfig, LabelOnDisk},
    Tool,
};
use image::{Rgba, RgbaImage};

#[derive(Debug, Clone, Copy)]
enum BBoxInput {
    None,
    Partial(Pos2),
    Finished(Pos2, Pos2),
}

pub struct Labeling<D: BoundrsDetection> {
    label_config: DynLabelConfig,
    current_label: Detections<D>,
    filter: Option<egui::TextureHandle>,
    mask_needs_update: bool,
    bbox_input: BBoxInput,
    repeat_mode: bool,
    region_delete_mode: bool,
    past_delete: usize,
    current_class: DynLabel,
    filter_opacity: u8,
    shown_classes: HashSet<usize>,
    last_time: f64,
}

impl<D: BoundrsDetection> Labeling<D> {
    pub fn new(label_config: DynLabelConfig, initial: &LabelOnDisk<D>) -> Self {
        let current_class = label_config
            .label_from_usize(0)
            .expect("At least 1 label is needed");
        let current_label = initial.load_label().unwrap();
        Labeling {
            label_config,
            current_label,
            filter: None,
            mask_needs_update: false,
            bbox_input: BBoxInput::None,
            repeat_mode: false,
            region_delete_mode: false,
            past_delete: 10,
            current_class,
            filter_opacity: 220,
            shown_classes: HashSet::new(),
            last_time: 0.0,
        }
    }
    fn set_mask(&mut self, ctx: &Context, img_rect: Rect) {
        let mask = generate_mask(
            &self.current_label,
            &self.shown_classes,
            img_rect,
            self.filter_opacity,
        );
        self.filter = Some(ctx.load_texture("mask", mask, egui::TextureOptions::LINEAR));
        self.mask_needs_update = false;
    }
    fn remove_labels(&mut self, pos: Pos2, img_rect: Rect) {
        self.current_label
            .0
            .retain(|label| !label.to_screen_rect(img_rect).contains(pos));
    }
    fn add_bb(&mut self, bb: D) {
        self.current_label.0.push(bb)
    }

    fn repeat_bbs_inside(
        &mut self,
        rect: Rect,
        img_rect: Rect,
        dataset: &Dataset<D>,
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
    fn delete_bbs_touching(
        &mut self,
        rect: Rect,
        img_rect: Rect,
        past: usize,
        dataset: &Dataset<D>,
    ) -> Result<()> {
        // get two coordinates to repeat only the labels completely in this box
        for datapoint in dataset.previous_datapoints(past) {
            let mut prev_label = datapoint.load_label()?;
            prev_label
                .0
                .retain(|label| !rect.intersects(label.to_screen_rect(img_rect)));
            datapoint.save_label(prev_label)?;
        }
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
    fn handle_img_response(&mut self, img_response: &Response, ui: &mut Ui, dataset: &Dataset<D>) {
        if img_response.secondary_clicked() {
            let screen_pos = img_response.interact_pointer_pos().unwrap();
            self.remove_bbs(screen_pos, img_response.rect);
            self.mask_needs_update = true;
            // self.update_mask(ui.ctx(), img_response.rect);
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
                let label = D::from_rect(label_rect, img_rect, &self.current_class);
                println!("{label:?}");
                self.add_bb(label);
                self.mask_needs_update = true;
                // self.update_mask(ui.ctx(), img_response.rect);
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
            } else if self.region_delete_mode {
                let rect = Rect::from_two_pos(pos1, pos2);
                self.delete_bbs_touching(rect, img_response.rect, self.past_delete, dataset)
                    .unwrap();
            } else {
                let img_rect = img_response.rect;
                let label_rect = Rect::from_two_pos(pos1, pos2);
                let label = D::from_rect(label_rect, img_rect, &self.current_class);
                println!("{label:?}");
                self.add_bb(label);
                self.mask_needs_update = true;
                // self.update_mask(ui.ctx(), img_response.rect);
                self.bbox_input = BBoxInput::None
            }
        }
    }
    fn class_pressed(&self, ui: &Ui) -> Option<DynLabel> {
        if ui.input(|i| i.time - self.last_time > 0.3) {
            return ui.input(|i| self.label_config.label_from_keys(&i.keys_down));
        }
        None
    }

    fn handle_class_keys(&mut self, ui: &Ui) {
        if let Some(class) = self.class_pressed(ui) {
            self.last_time = ui.input(|i| i.time);
            if self.filter.is_some() {
                if self.shown_classes.contains(&class.i) {
                    self.shown_classes.remove(&class.i);
                } else {
                    self.shown_classes.insert(class.i);
                }
                self.mask_needs_update = true;
            } else {
                self.current_class = class.clone();
            }
        }
    }
    fn handle_left_right(&mut self, ui: &Ui, dataset: &mut Dataset<D>) {
        let next_pressed = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
        let previous_pressed = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));

        let movement = match (next_pressed, previous_pressed, self.filter.is_some()) {
            (true, false, false) => DatasetMovement::Next,
            (false, true, false) => DatasetMovement::Previous,
            (true, false, true) => DatasetMovement::NextContaining(self.shown_classes.clone()),
            (false, true, true) => DatasetMovement::PreviousContaining(self.shown_classes.clone()),
            _ => return,
        };

        self.save_state(dataset.current()).unwrap();
        dataset.go(movement, None).unwrap();
        self.refresh_state(dataset.current()).unwrap();
    }
}

#[inline]
fn pos_inside_label_box<D: BoundrsDetection>(
    label: &Detections<D>,
    pos: Pos2,
    img_rect: Rect,
) -> bool {
    label
        .iter()
        .any(|l| l.to_screen_rect(img_rect).contains(pos))
}
fn generate_mask<D: BoundrsDetection>(
    label: &Detections<D>,
    shown_classes: &HashSet<usize>,
    img_rect: Rect,
    opacity: u8,
) -> ColorImage {
    let highlighted_label = label
        .iter()
        .cloned()
        .filter(|bb| shown_classes.contains(&bb.class_num()))
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

impl<D: BoundrsDetection> Tool<D> for Labeling<D> {
    fn draw_ui(&mut self, ui: &mut Ui) -> Result<()> {
        ui.horizontal(|ui| {
            ui.label("Filter opacity");
            if ui
                .add(Slider::new(&mut self.filter_opacity, 0..=255))
                .changed()
            {
                self.mask_needs_update = true;
            }
        });
        ui.vertical(|ui| {
            ui.add(Checkbox::new(
                &mut self.region_delete_mode,
                "Region delete Mode",
            ));
            ui.add(Checkbox::new(&mut self.repeat_mode, "Repeat Mode"));
        });
        ui.horizontal(|ui| {
            ui.label("Past delete");
            ui.add(DragValue::new(&mut self.past_delete));
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
        dataset: &mut Dataset<D>,
    ) -> Result<()> {
        if self.mask_needs_update && self.filter.is_some() {
            self.set_mask(central_panel.ctx(), img_response.rect)
        }
        if let Some(mask) = &self.filter {
            central_panel.put(img_response.rect, egui::Image::new(mask));
        }

        // Draw guides
        let pos = central_panel.input(|i| i.pointer.hover_pos());
        if let Some(pos) = pos {
            self.draw_guide(central_panel, pos)
        }
        self.draw_partial_box(central_panel);

        // Draw bbs
        self.draw_bbs(central_panel, img_response.rect);

        if img_response.has_focus() {
            // Handle filter mode
            if central_panel.input(|i| i.key_pressed(Key::F)) {
                if self.filter.is_some() {
                    self.filter = None
                } else {
                    self.set_mask(central_panel.ctx(), img_response.rect)
                }
            }

            self.handle_class_keys(central_panel);

            // Handle clicks for bbs
            self.handle_img_response(&img_response, central_panel, dataset);

            // Handle repeat button
            if central_panel.input(|i| i.key_pressed(Key::R)) {
                self.repeat_mode = !self.repeat_mode;
            }
            if central_panel.input_mut(|i| {
                i.consume_shortcut(&KeyboardShortcut {
                    modifiers: Modifiers::CTRL,
                    key: Key::D,
                })
            }) {
                self.repeat_mode = false;
                self.region_delete_mode = !self.region_delete_mode;
            }

            self.handle_left_right(central_panel, dataset);
        }
        Ok(())
    }
    fn refresh_state(&mut self, datapoint: &LabelOnDisk<D>) -> Result<()> {
        self.current_label = datapoint.load_label()?;
        self.mask_needs_update = self.filter.is_some();
        Ok(())
    }
    fn save_state(&self, datapoint: &LabelOnDisk<D>) -> Result<()> {
        datapoint.save_label(self.current_label.clone())
    }

    fn name(&self) -> &str {
        "Labeling Tool"
    }
}
