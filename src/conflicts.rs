use anyhow::Result;
use egui::*;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    dataset::{Dataset, DatasetMovement, DynLabel, DynLabelConfig, YoloBB, YoloLabel},
    SyncDatasets,
};
use image::{Rgba, RgbaImage};

// use clap::Parser;

// /// Simple program to greet a person
// #[derive(Parser, Debug)]
// #[command(author, version, about, long_about = None)]
// struct Args {
//     #[arg(short, long)]
//     gt_dir: PathBuf,

//     #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
//     pred_dirs: Vec<PathBuf>,

//     #[arg(long, default_value = "./labels_52.toml")]
//     gt_config: PathBuf,

//     #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
//     configs_pred: Vec<PathBuf>,
// }

#[derive(Debug)]
enum CurrentDataset {
    GroundTruth,
    Predicted(usize),
}
impl CurrentDataset {
    fn to_usize(&self) -> usize {
        match self {
            CurrentDataset::GroundTruth => 0,
            CurrentDataset::Predicted(i) => i + 1,
        }
    }
    fn from_usize(i: usize) -> Self {
        match i {
            0 => CurrentDataset::GroundTruth,
            i => CurrentDataset::Predicted(i - 1),
        }
    }
    fn offset(&mut self, offset: i32, max_size: usize) {
        let i = self.to_usize();
        let new_i = (i as i32 + offset).rem_euclid(max_size as i32);
        assert!(new_i >= 0);
        *self = CurrentDataset::from_usize(new_i as usize);
    }
    fn pretty_print(&self, pred_dirs: &[PathBuf]) -> String {
        match self {
            CurrentDataset::GroundTruth => "Ground Truth".into(),
            CurrentDataset::Predicted(i) => format!("Prediction in {:?}", pred_dirs[*i]),
        }
    }
}

pub struct Conflicts {
    pub gt_dataset: Dataset,
    gt_config: DynLabelConfig,
    // TODO store just the folder paths here and config in a separate dictionary
    pred_dirs: Vec<PathBuf>,
    pred_confs: HashMap<PathBuf, DynLabelConfig>,
    zoom: f32,
    // TODO move this to main app, pass new texture out of label / relabel function
    img_rect: Rect,
    filter: bool,
    filter_opacity: u8,
    last_time: f64,
    shown_classes: HashSet<usize>,
    current_dataset: CurrentDataset,
    current_label: YoloLabel,
    label_diffs: Vec<(String, i32)>,
}

impl Conflicts {
    pub fn from_dir<P: AsRef<Path>>(cc: &Context, dir: P) -> anyhow::Result<Self> {
        let mut gt_dataset_dir = PathBuf::from(dir.as_ref());
        gt_dataset_dir.push("normal");
        let gt_config = DynLabelConfig::load_from_file("labels_52.toml").unwrap();

        let mut confs = HashMap::new();
        let mut pred_dirs = vec![];
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry?;
            let subdir = entry.path();
            if subdir.is_dir() && subdir.file_name().unwrap().to_str().unwrap() != "normal" {
                pred_dirs.push(subdir.clone());
                let conf = if subdir
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .contains("yolov7")
                {
                    DynLabelConfig::load_from_file("labels_13.toml").unwrap()
                } else {
                    DynLabelConfig::load_from_file("labels_52.toml").unwrap()
                };
                confs.insert(subdir, conf);
            }
        }
        pred_dirs.sort();

        Conflicts::new(cc, &gt_dataset_dir, gt_config, pred_dirs, confs)
    }
    pub fn new(
        cc: &Context,
        gt_dataset_dir: &Path,
        gt_config: DynLabelConfig,
        pred_dirs: Vec<PathBuf>,
        pred_confs: HashMap<PathBuf, DynLabelConfig>,
    ) -> anyhow::Result<Self> {
        let shown_classes = HashSet::new();
        let gt_dataset = Dataset::from_input_dir(gt_dataset_dir)?;
        let image = gt_dataset.current_image()?;
        let current_bbs = gt_dataset.current_label()?;
        let mask = generate_mask(&current_bbs, &shown_classes, Rect::NOTHING, 250);
        // let mask_texture = cc.load_texture("mask", mask, egui::TextureOptions::LINEAR);

        Ok(Conflicts {
            gt_dataset,
            gt_config,
            zoom: 1.5,
            // mask_texture,
            img_rect: Rect::NOTHING,
            filter: false,
            filter_opacity: 250,
            last_time: 0.0,
            shown_classes: HashSet::new(),
            current_label: current_bbs.clone(),
            pred_dirs,
            pred_confs,
            current_dataset: CurrentDataset::GroundTruth,
            label_diffs: Vec::new(),
        })
    }
    fn get_current_config(&self) -> &DynLabelConfig {
        match self.current_dataset {
            CurrentDataset::GroundTruth => &self.gt_config,
            CurrentDataset::Predicted(i) => &self.pred_confs[&self.pred_dirs[i]],
        }
    }
    // TODO hacky, what is correct?
    fn label_differences(&self) -> Vec<(String, i32)> {
        let mut diff_counts: HashMap<_, i32> = HashMap::new();
        for bbox in &self.gt_dataset.current_label().unwrap() {
            let name = bbox.class(&self.gt_config).name[0..1].to_owned();
            *diff_counts.entry(name).or_default() += 1;
        }
        for bbox in &self.current_label {
            let name = bbox.class(self.get_current_config()).name[0..1].to_owned();
            *diff_counts.entry(name).or_default() -= 1;
        }
        let mut diffs: Vec<_> = diff_counts.into_iter().filter(|(_, v)| *v != 0).collect();
        diffs.sort();
        diffs
    }
    pub fn draw_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            let filename = self.gt_dataset.current_name();
            ui.label("Current image:");
            ui.label(filename);
        });
        ui.horizontal(|ui| {
            ui.label("Current dataset:");
            ui.label(self.current_dataset.pretty_print(&self.pred_dirs));
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
                "Go left and right: Left Arrow or A and Right Arrow or D",
            ));
        });
        ui.vertical(|ui| {
            ui.separator();
            ui.label(format!("{:?}", self.label_diffs));
        });
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
    //     let image = self.gt_dataset.current_image().unwrap();
    //     self.image_texture = ctx.load_texture("my-image", image, egui::TextureOptions::LINEAR);
    // }
    // fn update_mask(&mut self, ctx: &Context) {
    //     if self.filter {
    //         let mask = generate_mask(
    //             &self.current_label,
    //             &self.shown_classes,
    //             self.img_rect,
    //             self.filter_opacity,
    //         );
    //         // self.mask_texture = ctx.load_texture("mask", mask, egui::TextureOptions::LINEAR);
    //     }
    // }
    fn draw_bbs(&self, ui: &mut Ui) {
        let config = self.get_current_config();
        let img_rect = self.img_rect;
        let painter = ui.painter();
        // let size = self.img_rect.size();
        for bb in &self.current_label {
            if self
                .label_diffs
                .iter()
                .any(|(label, _)| label == &bb.class(self.get_current_config()).name[0..1])
            {
                let color = Color32::WHITE;
                let screen_rect = bb.to_screen_rect(img_rect);
                painter.rect_stroke(screen_rect, Rounding::ZERO, Stroke::new(5.0, color));
                let text_pos = screen_rect.left_bottom();
                self.draw_label_text(painter, text_pos, &bb.class(config));
            } else {
                let color = bb.class(config).color;
                let screen_rect = bb.to_screen_rect(img_rect);
                painter.rect_stroke(screen_rect, Rounding::ZERO, Stroke::new(2.0, color));
                let text_pos = screen_rect.left_bottom();
                self.draw_label_text(painter, text_pos, &bb.class(config));
            }
        }
    }
    fn draw_label_text(&self, painter: &Painter, text_pos: Pos2, class: &DynLabel) {
        painter.rect(
            Rect::from_two_pos(text_pos, text_pos + [40.0, -35.0].into()),
            Rounding::ZERO,
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
            // self.update_mask(ui.ctx());
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
                // self.update_mask(ctx);
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
        self.gt_dataset.go(movement.clone(), None).unwrap();
        self.current_label = self.gt_dataset.current_label().unwrap();
        self.current_dataset = CurrentDataset::GroundTruth; // otherwise need to find current_label in special way
                                                            // self.update_texture(ctx);
                                                            // self.update_mask(ctx);
    }

    fn current_label(&mut self) -> Result<Option<YoloLabel>> {
        let label = match self.current_dataset {
            CurrentDataset::GroundTruth => self.gt_dataset.current_label()?,
            CurrentDataset::Predicted(i) => {
                // Here we know the file exists because of the code above
                let mut gt_name: PathBuf = self.gt_dataset.current_name().into();
                gt_name.set_extension("txt");
                let path = &self.pred_dirs[i];
                let mut label_file = path.clone();
                label_file.push(gt_name);
                if !label_file.exists() {
                    return Ok(None);
                }
                assert!(label_file.exists());
                let yolo_strs = std::fs::read_to_string(label_file)?;

                let mut labels = vec![];
                for line in yolo_strs.lines() {
                    let label = YoloBB::from_str(line)?;
                    labels.push(label)
                }
                labels
            }
        };
        Ok(Some(label))
    }

    fn move_predicitions(&mut self, movement: PredictionMovement) {
        let offset = match movement {
            PredictionMovement::Next => 1,
            PredictionMovement::Previous => -1,
        };
        self.current_dataset
            .offset(offset, self.pred_dirs.len() + 1);
        loop {
            match self.current_label().unwrap() {
                Some(label) => {
                    self.current_label = label;
                    break;
                }
                None => self
                    .current_dataset
                    .offset(offset, self.pred_dirs.len() + 1),
            };
        }
        self.label_diffs = self.label_differences();
    }
    fn handle_prediction_switch(&mut self, ctx: &Context) {
        // We save the current label and update the state in the new mode
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            self.move_predicitions(PredictionMovement::Next);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            self.move_predicitions(PredictionMovement::Previous);
        }
    }
    pub fn draw_central_panel(&mut self, ctx: &Context, ui: &mut Ui) {
        self.handle_prediction_switch(ctx);
        // let img_response = self.draw_img(ui);

        // filter
        // if self.filter {
        //     ui.put(
        //         self.img_rect,
        //         egui::Image::new(&self.mask_texture, self.mask_texture.size_vec2()),
        //     );
        // }

        // Draw bbs
        self.draw_bbs(ui);
        // Handle prev next picture keyboard
        self.handle_left_right(ctx);
        // Handle clicks for bbs
        // self.handle_img_response(img_response, ui);
        // Handle class setting
        self.handle_class_keys(ctx);
        // Handle filter mode
        let filter_pressed = ctx.input(|i| i.key_pressed(egui::Key::F));
        if filter_pressed {
            self.filter = !self.filter;
        }
    }
    pub fn prepare_switch(&mut self) -> SyncDatasets {
        let (_, current_pos, _) = self.gt_dataset.get_progress();
        SyncDatasets { current_pos }
    }
    pub fn refresh_after_switch(&mut self, sync: &SyncDatasets, ctx: &Context) {
        let SyncDatasets { current_pos } = sync;
        let movement = DatasetMovement::JumpTo(*current_pos);
        self.gt_dataset.go(movement, None).unwrap();
        self.current_label = self.gt_dataset.current_label().unwrap();
        // TODO avoid doing this here
        self.current_dataset = CurrentDataset::GroundTruth;
        // self.update_texture(ctx);
        // self.update_mask(ctx);
    }
}

enum PredictionMovement {
    Next,
    Previous,
}

#[inline]
fn pos_inside_label_box(label: &YoloLabel, pos: Pos2, img_rect: Rect) -> bool {
    label
        .iter()
        .any(|l| l.to_screen_rect(img_rect).contains(pos))
}
// TODO move this to utils in the lib and reuse it with labeling. Or encapsulate all funtionality as a tool or smth
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
