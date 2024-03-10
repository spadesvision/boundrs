use std::{fs::OpenOptions, path::PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers, Ui};
use log::info;
use serde::{Deserialize, Serialize};

use anyhow::Result;

use crate::{
    dataset::{BoundrsDataset, BoundrsDetection, DatasetMovement, LabelOnDisk},
    Tool,
};

#[derive(Debug, Deserialize, Serialize)]
struct TagEntry {
    image: String,
    tag: String,
}

#[derive(Default)]
pub struct Tagging {
    data: TaggingToolData,
    previous_tag: Option<String>,
}

#[derive(Default)]
struct TaggingToolData {
    input: String,
    current_tags: Vec<String>,
}

impl TaggingToolData {
    fn draw_tags(&mut self, ui: &mut Ui) {
        ui.label("Current Tags:");
        ui.horizontal_wrapped(|ui| {
            let mut to_remove = None;
            for (index, tag) in self.current_tags.iter().enumerate() {
                if ui.button(tag).secondary_clicked() {
                    to_remove = Some(index);
                }
            }
            if let Some(index) = to_remove {
                info!("Removing tag nr {index}");
                self.current_tags.remove(index);
            }
        });
    }
    fn load_tags(&mut self, current_label_jpg: &str) -> Result<()> {
        if !PathBuf::from("tags.csv").exists() {
            let mut wtr = csv::Writer::from_path("tags.csv")?;
            wtr.write_record(["image", "tag"])?;
            wtr.flush()?;
        }
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path("tags.csv")?;
        self.current_tags.clear();
        let mut other_tags = vec![];
        for result in rdr.deserialize() {
            let record: TagEntry = result?;
            if record.image == current_label_jpg {
                self.current_tags.push(record.tag);
            } else {
                other_tags.push(record);
            }
        }
        // Overwriting tags.csv with the other_tags
        // println!("Saving {:?} tags", other_tags);
        println!("Saving {} tags", other_tags.len());
        let mut wtr = csv::Writer::from_path("tags.csv")?;
        if other_tags.is_empty() {
            wtr.write_record(["image", "tag"])?;
        }
        for tag_entry in other_tags {
            wtr.serialize(tag_entry)?;
        }
        wtr.flush()?;
        Ok(())
    }
    fn save_tags(&self, current_label_jpg: &str) -> Result<()> {
        let file = OpenOptions::new()
            .append(true)
            .create(true) // Creates the file if it does not exist
            .open("tags.csv")?;

        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);
        for tag in &self.current_tags {
            let entry = TagEntry {
                image: current_label_jpg.to_string(),
                tag: tag.to_string(),
            };
            wtr.serialize(entry)?;
        }
        println!("Saving additional {} tags", self.current_tags.len());
        wtr.flush()?;
        Ok(())
    }
}

impl Tagging {
    // fn handle_keys(&mut self, ctx: &Ui, current_jpg: &str) -> Result<()> {
    //     if !self.is_open && ctx.input(|i| i.key_pressed(egui::Key::T)) {
    //         self.data.load_tags(current_jpg)?;
    //         self.is_open = true;
    //     }
    //     if self.is_open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
    //         self.data.save_tags(current_jpg)?;
    //         self.is_open = false;
    //     }
    //     Ok(())
    // }

    fn draw_ui(&mut self, ui: &mut Ui) -> Result<()> {
        // self.handle_keys(ui, current_jpg)?;
        // egui::Window::new("Tagging Tool")
        //     // .open(&mut self.is_open)
        //     .show(ui.ctx(), |ui| {
        // show the current tags
        self.data.draw_tags(ui);

        let response = ui.text_edit_singleline(&mut self.data.input);
        // response.request_focus();
        if response.gained_focus() {
            println!("gained focus");
        }

        // Add other UI elements for tagging as needed
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            println!("Adding tag {}", self.data.input);
            self.data.current_tags.push(self.data.input.clone());
            self.previous_tag = Some(self.data.input.clone());
            self.data.input = "".to_string();
        }
        Ok(())
        // response.request_focus();
        //     });
        // Ok(())
    }
    fn handle_left_right<D: BoundrsDetection>(&mut self, ui: &Ui, dataset: &mut BoundrsDataset<D>) {
        let next_pressed = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
        let previous_pressed = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));

        let movement = match (next_pressed, previous_pressed) {
            (true, false) => DatasetMovement::Next,
            (false, true) => DatasetMovement::Previous,
            _ => return,
        };

        self.save_state(dataset.current()).unwrap();
        dataset.go(movement, None).unwrap();
        self.refresh_state(dataset.current()).unwrap();
    }

    fn repeat_tags(&mut self) {
        info!("Repeating tags");
        if let Some(prev_tag) = &self.previous_tag {
            self.data.current_tags.push(prev_tag.clone())
        }
    }
}

impl<D: BoundrsDetection> Tool<D> for Tagging {
    fn draw_ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        self.draw_ui(ui)
    }

    fn draw_in_central_panel(
        &mut self,
        central_panel: &mut Ui,
        img_response: egui::Response,
        dataset: &mut crate::dataset::BoundrsDataset<D>,
    ) -> anyhow::Result<()> {
        if img_response.has_focus() {
            self.handle_left_right(central_panel, dataset)
        }
        if central_panel.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut {
                modifiers: Modifiers::NONE,
                logical_key: Key::R,
            })
        }) {
            self.repeat_tags()
        }
        Ok(())
    }

    fn refresh_state(&mut self, datapoint: &LabelOnDisk<D>) -> anyhow::Result<()> {
        self.data.load_tags(datapoint.img_name())
    }

    fn save_state(&self, datapoint: &LabelOnDisk<D>) -> anyhow::Result<()> {
        self.data.save_tags(datapoint.img_name())
    }

    fn name(&self) -> &str {
        "Tagging"
    }
}
