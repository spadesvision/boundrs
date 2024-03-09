use anyhow::{anyhow, Result};
use egui::*;
use glob::glob;
use serde::Deserialize;
// use serde::Deserialize;
// use serde::Serialize;
// use serde_json;
use std::collections::HashSet;
use std::fs::File;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
// use std::str::FromStr;

use bje_detections::{Card, Detection, Detections, YoloBBox};

#[derive(Deserialize, Clone)]
pub struct DynLabelSpec {
    keys: HashSet<Key>,
    name: String,
}

impl DynLabelSpec {
    pub fn into_label(self, i: usize, color: Color32) -> DynLabel {
        DynLabel {
            i,
            name: self.name,
            color,
            keys: self.keys,
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct DynLabelConfig {
    labels: Vec<DynLabelSpec>,
}

impl DynLabelConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let toml_str = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&toml_str)?)
    }
    pub fn color_for(&self, i: usize) -> Color32 {
        match i % 13 {
            0 => Color32::from_rgb(0x2f, 0x4f, 0x4f),
            1 => Color32::from_rgb(0x8b, 0x45, 0x13),
            2 => Color32::from_rgb(0x00, 0x80, 0x00),
            3 => Color32::from_rgb(0x4b, 0x00, 0x82),
            4 => Color32::from_rgb(0xff, 0x00, 0x00),
            5 => Color32::from_rgb(0xff, 0xff, 0x00),
            6 => Color32::from_rgb(0x00, 0xff, 0x00),
            7 => Color32::from_rgb(0x00, 0xff, 0xff),
            8 => Color32::from_rgb(0x00, 0x00, 0xff),
            9 => Color32::from_rgb(0xff, 0x00, 0xff),
            10 => Color32::from_rgb(0x64, 0x95, 0xed),
            11 => Color32::from_rgb(0xff, 0xda, 0xb9),
            12 => Color32::from_rgb(0xff, 0x69, 0xb6),
            _ => unreachable!(),
        }
    }
    pub fn label_from_usize(&self, i: usize) -> Option<DynLabel> {
        self.labels
            .get(i)
            .map(|spec| spec.clone().into_label(i, self.color_for(i)))
    }
    // pub fn keybindings(&self) -> impl Iterator<Item = HashSet<Key>> {
    //     self.labels.clone().into_iter().map(|l| l.keys)
    // }
    pub fn num_labels(&self) -> usize {
        self.labels.len()
    }
    pub fn label_from_keys(&self, keys: &HashSet<Key>) -> Option<DynLabel> {
        // self.labels.iter().filter(|l| l.keys.is_subset(keys)).map().next()
        for (i, spec) in self.labels.iter().enumerate() {
            if spec.keys.is_subset(keys) {
                return self.label_from_usize(i);
            }
        }
        None
    }
}

// pub trait DatasetLabel
// where
//     Self: std::fmt::Debug + Clone + Copy + PartialEq + Eq + std::hash::Hash,
// {
//     fn color(self) -> Color32;
//     fn keys_to_class(keys: HashSet<Key>) -> Option<Self> {
//         keys.into_iter()
//             .filter_map(|key| Self::key_to_class(key))
//             .next()
//     }
//     fn key_to_class(key: Key) -> Option<Self>;
//     fn from_usize(i: usize) -> Self;
//     fn to_usize(self) -> usize;
//     fn to_name(self) -> String;
// }

#[derive(Debug, Clone)]
pub struct DynLabel {
    pub i: usize,
    pub name: String,
    pub color: Color32,
    pub keys: HashSet<Key>,
}

pub trait BoundrsDetection: Detection {
    fn class(&self, config: &DynLabelConfig) -> DynLabel;
    fn class_num(&self) -> usize;
    fn to_screen_rect(self, img_rect: Rect) -> Rect;
    fn is_close(&self, other: &Self, threshold: f32) -> bool;
    fn iou(&self, other: &Self) -> f32;
    fn x(&self) -> f32;
    fn from_rect(rect: Rect, img_rect: Rect, class: &DynLabel) -> Self;
}

// pub type YoloLabel<D: Detection> = Vec<D>;

// #[derive(Debug, Clone, Copy, PartialEq)]
// pub struct YoloBB {
//     pub class_num: usize,
//     pub x: f32,
//     y: f32,
//     w: f32,
//     h: f32,
// }

// impl FromStr for YoloBB {
//     type Err = Error;

//     fn from_str(s: &str) -> Result<Self> {
//         let parts: Vec<_> = s.split(' ').collect();
//         let class_num: usize = parts[0].parse()?;
//         let (x, y) = (parts[1].parse()?, parts[2].parse()?);
//         let (w, h) = (parts[3].parse()?, parts[4].parse()?);
//         Ok(Self {
//             class_num,
//             x,
//             y,
//             w,
//             h,
//         })
//     }
// }

impl BoundrsDetection for YoloBBox {
    fn iou(&self, other: &YoloBBox) -> f32 {
        let rect_0_1: Rect = [[0.0, 0.0].into(), [1.0, 1.0].into()].into();
        let self_box = self.to_screen_rect(rect_0_1);
        let other_box = other.to_screen_rect(rect_0_1);
        self_box.intersect(other_box).area() / self_box.union(other_box).area()
    }
    fn is_close(&self, other: &YoloBBox, threshold: f32) -> bool {
        let rect_0_1: Rect = [[0.0, 0.0].into(), [1.0, 1.0].into()].into();
        let self_box = self.to_screen_rect(rect_0_1);
        let other_box = other.to_screen_rect(rect_0_1);
        let iou = self_box.intersect(other_box).area() / self_box.union(other_box).area();
        self.class == other.class && iou > threshold
    }
    fn to_screen_rect(self, img_rect: Rect) -> Rect {
        let img_w = img_rect.size().x;
        let img_h = img_rect.size().y;
        let yl = self;
        let rect = Rect::from_center_size(
            [yl.x * img_w, yl.y * img_h].into(),
            [yl.w * img_w, yl.h * img_h].into(),
        );
        rect.translate(img_rect.left_top().to_vec2())
    }
    fn class(&self, config: &DynLabelConfig) -> DynLabel {
        // TODO improve error handling, or even better, encode in type system
        config
            .label_from_usize(self.class.to_usize())
            .unwrap_or_else(|| {
                println!("{:?}", self);
                panic!(
                    "Config should work with this label, {:?} > {}",
                    self.class,
                    config.num_labels()
                )
            })
    }
    fn from_rect(rect: Rect, img_rect: Rect, class: &DynLabel) -> Self {
        let rect = rect.intersect(img_rect);
        let rect = rect.translate(-img_rect.left_top().to_vec2());
        let img_size = img_rect.size();
        let img_w = img_size.x;
        let img_h = img_size.y;
        let center = rect.center();
        let x = center.x / img_w;
        let y = center.y / img_h;
        let size = rect.size();
        let w = size.x / img_w;
        let h = size.y / img_h;
        let class_num = class.i;
        YoloBBox {
            class: Card::from_usize(class_num).unwrap(),
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            w,
            h,
            confidence: 1.0,
        }
    }

    fn class_num(&self) -> usize {
        self.class.to_usize()
    }

    fn x(&self) -> f32 {
        self.x
    }
}

#[derive(Debug, Clone)]
pub struct LabelOnDisk<D: Detection> {
    img_src: PathBuf,
    label_src: PathBuf,
    detections: PhantomData<D>,
}

fn load_image_from_path(path: &std::path::Path) -> Result<ColorImage> {
    let image = image::io::Reader::open(path)?.decode()?;
    let size = [image.width() as _, image.height() as _];
    let image_buffer = image.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    Ok(ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()))
}

impl<D: BoundrsDetection> LabelOnDisk<D> {
    pub fn new(img_src: PathBuf, labels_dir: PathBuf) -> Self {
        let img_src = img_src
            .file_name()
            .expect(".jpg extension should be a file")
            .into();
        let mut label_src = labels_dir;
        label_src.push(&img_src);
        label_src.set_extension("txt");
        LabelOnDisk {
            img_src,
            label_src,
            detections: PhantomData::<D>,
        }
    }
    pub fn load_image(&self) -> Result<ColorImage> {
        load_image_from_path(&self.img_src)
    }
    pub fn load_label(&self) -> Result<Detections<D>> {
        let label_src = &self.label_src;
        if label_src.exists() {
            File::create(&self.label_src).expect("File creation should not fail");
        }

        Detections::from_file(&self.label_src)
    }
    pub fn save_label(&self, label: Detections<D>) -> Result<()> {
        label.write_to_file(&self.label_src)?;
        Ok(())
    }
    pub fn img_name(&self) -> &str {
        self.img_src
            .file_name()
            .expect(".jpg path should be a file")
            .to_str()
            .expect(".jpg filename can be made to a string")
    }
    pub fn label_name(&self) -> &str {
        self.label_src
            .file_name()
            .expect(".tx path should be a file")
            .to_str()
            .expect(".txt filename can be made to a string")
    }

    pub(crate) fn remove_prefix(&self, old_prefix: &str) -> Self {
        let label_name = self.label_name();
        let label_name = label_name.replace(old_prefix, "");
        Self {
            label_src: self.label_src.with_file_name(label_name),
            img_src: self.img_src.clone(),
            detections: PhantomData,
        }
    }
    pub(crate) fn add_prefix(&self, new_prefix: &str) -> Self {
        let label_name = self.label_name();
        let label_name = format!("{new_prefix}{label_name}");
        Self {
            label_src: self.label_src.with_file_name(label_name),
            img_src: self.img_src.clone(),
            detections: PhantomData,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DatasetMovement {
    Next,
    Previous,
    NextContaining(HashSet<usize>),
    PreviousContaining(HashSet<usize>),
    JumpTo(usize),
}

pub struct Dataset<D: BoundrsDetection> {
    data: Vec<LabelOnDisk<D>>,
    i: usize,
}

impl<D: BoundrsDetection> Dataset<D> {
    pub fn from_input_dir(labels_dir: &Path) -> Result<Self> {
        let mut data = vec![];
        // let labels_dir = PathBuf::from("./input");
        let jpg_glob = format!("{}/*.jpg", labels_dir.to_str().unwrap());
        let mut paths: Vec<_> = glob(&jpg_glob)?.filter_map(Result::ok).collect();
        alphanumeric_sort::sort_path_slice(&mut paths);
        for img_src in paths.into_iter() {
            data.push(LabelOnDisk::new(img_src, labels_dir.to_path_buf()))
        }
        if data.is_empty() {
            return Err(anyhow!(
                "Dataset is empty. You need to put .jpg files in ./input/"
            ));
        }
        // start at first imgage without labels
        let first_no_label = data
            .iter()
            .position(|p| !p.label_src.is_file())
            .unwrap_or(0);

        Ok(Dataset {
            data,
            i: first_no_label,
        })
    }
    pub fn with_prefix(labels_dir: &Path, prefix: &str) -> Result<Self> {
        let mut dataset = Dataset::from_input_dir(labels_dir)?;
        for datapoint in &mut dataset.data {
            let label_name = datapoint
                .label_src
                .file_name()
                .expect(".txt path should be a file")
                .to_str()
                .expect(".txt filename should be a &str");
            let label_prefix_name = format!("{prefix}{label_name}");
            datapoint.label_src = datapoint.label_src.with_file_name(label_prefix_name);
        }
        Ok(dataset)
    }

    pub fn current_image(&self) -> Result<ColorImage> {
        self.current().load_image()
    }
    pub fn current_label(&self) -> Result<Detections<D>> {
        self.current().load_label()
    }
    pub fn previous_labels(&self, num: usize) -> Result<Vec<Detections<D>>> {
        (1..=num)
            .map(|back| {
                let previous = self.i.saturating_sub(back);
                self.data[previous].load_label()
            })
            .collect()
    }
    pub fn previous_datapoints(&self, num: usize) -> Vec<&LabelOnDisk<D>> {
        (0..=num)
            .map(|back| {
                let previous = self.i.saturating_sub(back);
                &self.data[previous]
            })
            .collect()
    }
    pub fn current_name(&self) -> &str {
        self.current().img_name()
    }
    pub fn current_img_path(&self) -> &PathBuf {
        &self.current().img_src
    }
    pub fn current_img_uri(&self) -> String {
        format!("file://{}", self.current_img_path().display())
    }
    pub fn get_progress(&self) -> (usize, usize, usize) {
        (0, self.i, self.data.len())
    }
    fn save_label(&self, label: Detections<D>) -> Result<()> {
        self.data[self.i].save_label(label)
    }
    fn next(&mut self) -> Result<()> {
        self.i = std::cmp::min(self.i + 1, self.data.len() - 1);
        Ok(())
    }
    fn previous(&mut self) -> Result<()> {
        self.i = self.i.saturating_sub(1);
        Ok(())
    }
    fn next_containing(&mut self, classes: &HashSet<usize>) -> Result<()> {
        while self.i < self.data.len() - 1 {
            self.i += 1;
            if self.data[self.i].label_src.exists() {
                let label = self.data[self.i].load_label()?;
                if label.0.iter().any(|bb| classes.contains(&bb.class_num())) {
                    break;
                }
            }
        }
        Ok(())
    }
    fn previous_containing(&mut self, classes: &HashSet<usize>) -> Result<()> {
        while self.i > 0 {
            self.i -= 1;
            if self.data[self.i].label_src.exists() {
                let label = self.data[self.i].load_label()?;
                if label.0.iter().any(|bb| classes.contains(&bb.class_num())) {
                    break;
                }
            }
        }
        Ok(())
    }
    fn go_to(&mut self, pos: usize) -> Result<()> {
        self.i = pos.clamp(0, self.data.len() - 1);
        Ok(())
    }

    // TOD take Option for label instead of save flag
    pub fn go(
        &mut self,
        movement: DatasetMovement,
        save_label: Option<Detections<D>>,
    ) -> Result<()> {
        if let Some(label) = save_label {
            self.save_label(label)?;
        }
        match movement {
            DatasetMovement::Next => self.next(),
            DatasetMovement::Previous => self.previous(),
            DatasetMovement::NextContaining(classes) => self.next_containing(&classes),
            DatasetMovement::PreviousContaining(classes) => self.previous_containing(&classes),
            DatasetMovement::JumpTo(pos) => self.go_to(pos),
        }
    }

    pub fn current(&self) -> &LabelOnDisk<D> {
        &self.data[self.i]
    }

    pub fn iter_data(&self) -> impl Iterator<Item = &LabelOnDisk<D>> {
        self.data.iter()
    }
}
