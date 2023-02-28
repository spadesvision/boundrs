use crate::egui::*;
use anyhow::{Error, Result};
use glob::glob;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::prelude::*;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::FromStr;

pub trait DatasetLabel
where
    Self: std::fmt::Debug + Clone + Copy + PartialEq + Eq + std::hash::Hash,
{
    fn color(self) -> Color32;
    fn shortcuts() -> HashMap<Vec<Key>, Self>
    where
        Self: Sized;
    fn from_usize(i: usize) -> Self;
    fn to_usize(self) -> usize;
    fn to_name(self) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    A = 0,
    K,
    Q,
    J,
    V10,
    V9,
    V8,
    V7,
    V6,
    V5,
    V4,
    V3,
    V2,
}
impl DatasetLabel for Card {
    fn color(self) -> Color32 {
        use Card::*;
        match self {
            A => Color32::from_rgb(0x2f, 0x4f, 0x4f),
            K => Color32::from_rgb(0x8b, 0x45, 0x13),
            Q => Color32::from_rgb(0x00, 0x80, 0x00),
            J => Color32::from_rgb(0x4b, 0x00, 0x82),
            V10 => Color32::from_rgb(0xff, 0x00, 0x00),
            V9 => Color32::from_rgb(0xff, 0xff, 0x00),
            V8 => Color32::from_rgb(0x00, 0xff, 0x00),
            V7 => Color32::from_rgb(0x00, 0xff, 0xff),
            V6 => Color32::from_rgb(0x00, 0x00, 0xff),
            V5 => Color32::from_rgb(0xff, 0x00, 0xff),
            V4 => Color32::from_rgb(0x64, 0x95, 0xed),
            V3 => Color32::from_rgb(0xff, 0xda, 0xb9),
            V2 => Color32::from_rgb(0xff, 0x69, 0xb6),
        }
    }

    fn shortcuts() -> HashMap<Vec<Key>, Card> {
        let mut map = HashMap::new();
        map.insert(vec![Key::Num1], Card::A);
        map.insert(vec![Key::Num2], Card::V2);
        map.insert(vec![Key::Num3], Card::V3);
        map.insert(vec![Key::Num4], Card::V4);
        map.insert(vec![Key::Num5], Card::V5);
        map.insert(vec![Key::Num6], Card::V6);
        map.insert(vec![Key::Num7], Card::V7);
        map.insert(vec![Key::Num8], Card::V8);
        map.insert(vec![Key::Num9], Card::V9);
        map.insert(vec![Key::Num0], Card::V10);
        map.insert(vec![Key::J], Card::J);
        map.insert(vec![Key::Q], Card::Q);
        map.insert(vec![Key::K], Card::K);
        map
    }

    fn from_usize(i: usize) -> Card {
        use Card::*;
        match i {
            0 => A,
            1 => K,
            2 => Q,
            3 => J,
            4 => V10,
            5 => V9,
            6 => V8,
            7 => V7,
            8 => V6,
            9 => V5,
            10 => V4,
            11 => V3,
            12 => V2,
            _ => unreachable!(),
        }
    }
    fn to_usize(self) -> usize {
        use Card::*;
        match self {
            A => 0,
            K => 1,
            Q => 2,
            J => 3,
            V10 => 4,
            V9 => 5,
            V8 => 6,
            V7 => 7,
            V6 => 8,
            V5 => 9,
            V4 => 10,
            V3 => 11,
            V2 => 12,
        }
    }

    fn to_name(self) -> String {
        use Card::*;
        match self {
            A => "A".into(),
            K => "K".into(),
            Q => "Q".into(),
            J => "J".into(),
            V10 => "10".into(),
            V9 => "9".into(),
            V8 => "8".into(),
            V7 => "7".into(),
            V6 => "6".into(),
            V5 => "5".into(),
            V4 => "4".into(),
            V3 => "3".into(),
            V2 => "2".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Suit {
    Hearts = 0,
    Diamonds,
    Clubs,
    Spades,
}

impl DatasetLabel for Suit {
    fn color(self) -> Color32 {
        use Suit::*;
        match self {
            Hearts => Color32::from_rgb(0x2f, 0x4f, 0x4f),
            Diamonds => Color32::from_rgb(0x8b, 0x45, 0x13),
            Clubs => Color32::from_rgb(0x00, 0x80, 0x00),
            Spades => Color32::from_rgb(0x4b, 0x00, 0x82),
        }
    }

    fn shortcuts() -> HashMap<Vec<Key>, Suit> {
        use Suit::*;
        let mut map = HashMap::new();
        map.insert(vec![Key::H], Hearts);
        map.insert(vec![Key::D], Diamonds);
        map.insert(vec![Key::C], Clubs);
        map.insert(vec![Key::S], Spades);
        map
    }
    fn from_usize(i: usize) -> Suit {
        use Suit::*;
        // println!("suit from {i}");
        match i {
            0 => Hearts,
            1 => Diamonds,
            2 => Clubs,
            3 => Spades,
            _ => unreachable!(),
        }
    }
    fn to_usize(self) -> usize {
        use Suit::*;
        match self {
            Hearts => 0,
            Diamonds => 1,
            Clubs => 2,
            Spades => 3,
        }
    }
    fn to_name(self) -> String {
        use Suit::*;
        match self {
            Hearts => "H".into(),
            Diamonds => "D".into(),
            Clubs => "C".into(),
            Spades => "S".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CardSuit(pub Card, pub Suit);

impl DatasetLabel for CardSuit {
    fn color(self) -> Color32 {
        self.0.color()
    }
    fn shortcuts() -> HashMap<Vec<Key>, CardSuit> {
        let card_shortcuts = Card::shortcuts();
        let suit_shortcuts = Suit::shortcuts();
        let mut shortcuts = HashMap::new();
        for (cs, card) in card_shortcuts.iter() {
            for (ss, suit) in suit_shortcuts.iter() {
                let mut shortcut = ss.clone();
                shortcut.append(&mut cs.clone());
                shortcuts.insert(shortcut, CardSuit(*card, *suit));
            }
        }
        shortcuts
    }
    fn from_usize(i: usize) -> CardSuit {
        // println!("CardSuit from usize {i}");
        let suit_usize = i / 13;
        let card_usize = i % 13;
        let suit = Suit::from_usize(suit_usize);
        let card = Card::from_usize(card_usize);
        CardSuit(card, suit)
    }
    fn to_usize(self) -> usize {
        let (card, suit) = (self.0, self.1);
        let card_usize = card.to_usize();
        let suit_usize = suit.to_usize();
        13 * suit_usize + card_usize
    }
    fn to_name(self) -> String {
        let (card, suit) = (self.0, self.1);
        format!("{}{}", card.to_name(), suit.to_name())
    }
}

pub type YoloLabel<L> = Vec<YoloBB<L>>;

#[derive(Debug, Clone, Copy)]
pub struct YoloBB<L: DatasetLabel> {
    class_num: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: PhantomData<L>,
}

impl<L: DatasetLabel> FromStr for YoloBB<L> {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.split(' ').collect();
        let class_num: usize = parts[0].parse()?;
        let (x, y) = (parts[1].parse()?, parts[2].parse()?);
        let (w, h) = (parts[3].parse()?, parts[4].parse()?);
        Ok(Self {
            class_num,
            x,
            y,
            w,
            h,
            label: PhantomData::default(),
        })
    }
}

impl<L: DatasetLabel> YoloBB<L> {
    fn as_string(self) -> String {
        format!(
            "{} {} {} {} {}",
            self.class_num, self.x, self.y, self.w, self.h
        )
    }
}

pub trait BoundingBox<L: DatasetLabel> {
    // fn rect(&self, size: Vec2) -> Rect;
    fn to_screen_rect(&self, rect: Rect) -> Rect;
    fn class(&self) -> L;
    fn from_rect(rect: Rect, size: Rect, class: L) -> Self;
}

impl<L: DatasetLabel> BoundingBox<L> for YoloBB<L> {
    // fn rect(&self, size: Vec2) -> Rect {
    //     let img_w = size.x;
    //     let img_h = size.y;
    //     let yl = self;
    //     Rect::from_center_size(
    //         [yl.x * img_w, yl.y * img_h].into(),
    //         [yl.w * img_w, yl.h * img_h].into(),
    //     )
    // }
    fn to_screen_rect(&self, img_rect: Rect) -> Rect {
        let img_w = img_rect.size().x;
        let img_h = img_rect.size().y;
        let yl = self;
        let rect = Rect::from_center_size(
            [yl.x * img_w, yl.y * img_h].into(),
            [yl.w * img_w, yl.h * img_h].into(),
        );
        rect.translate(img_rect.left_top().to_vec2())
    }
    fn class(&self) -> L {
        L::from_usize(self.class_num)
    }
    fn from_rect(rect: Rect, img_rect: Rect, class: L) -> Self {
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
        let class_num = class.to_usize();
        YoloBB {
            class_num,
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            w,
            h,
            label: PhantomData::default(),
        }
    }
}

#[derive(Debug)]
struct Datapoint<L: DatasetLabel> {
    img_src: PathBuf,
    label_src: PathBuf,
    label: PhantomData<L>,
}

fn load_image_from_path(path: &std::path::Path) -> Result<ColorImage> {
    let image = image::io::Reader::open(path)?.decode()?;
    let size = [image.width() as _, image.height() as _];
    let image_buffer = image.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    Ok(ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()))
}

impl<L: DatasetLabel> Datapoint<L> {
    fn new(img_src: PathBuf, labels_dir: PathBuf) -> Self {
        let img_filename = img_src.file_name().unwrap();
        let mut label_src = labels_dir;
        label_src.push(img_filename);
        label_src.set_extension("txt");
        Datapoint::<L> {
            img_src,
            label_src,
            label: PhantomData::default(),
        }
    }
    fn load_image(&self) -> Result<ColorImage> {
        load_image_from_path(&self.img_src)
    }
    fn load_label(&self) -> Result<YoloLabel<L>> {
        if !self.label_src.exists() {
            File::create(&self.label_src).unwrap();
        }
        let yolo_strs = std::fs::read_to_string(&self.label_src)?;

        let mut labels = vec![];
        for line in yolo_strs.lines() {
            let label = YoloBB::from_str(line)?;
            labels.push(label)
        }
        Ok(labels)
    }
    fn save_label(&self, label: YoloLabel<L>) -> Result<()> {
        let mut file = File::create(&self.label_src)?;
        for yolo_label in label {
            writeln!(file, "{}", yolo_label.as_string())?;
        }
        println!("Saving labels to {:?}", self.label_src);
        Ok(())
    }
    fn name(&self) -> String {
        self.img_src.file_name().unwrap().to_str().unwrap().into()
    }
}

#[derive(Clone, PartialEq)]
pub enum DatasetMovement<L: DatasetLabel> {
    Next,
    Previous,
    NextContaining(HashSet<L>),
    PreviousContaining(HashSet<L>),
    JumpTo(usize),
}

pub struct Dataset<L: DatasetLabel> {
    data: Vec<Datapoint<L>>,
    i: usize,
}

impl<L: DatasetLabel> Dataset<L> {
    pub fn from_input_dir() -> Result<Self> {
        let mut data = vec![];
        let labels_dir = PathBuf::from("./input");
        let mut paths: Vec<_> = glob("./input/*.jpg")?.map(|p| p.unwrap()).collect();
        alphanumeric_sort::sort_path_slice(&mut paths);
        for img_src in paths.into_iter() {
            data.push(Datapoint::new(img_src, labels_dir.clone()))
        }
        // start at first imgage without labels
        let first_no_label = data
            .iter()
            .position(|p| !p.label_src.is_file())
            .unwrap_or(0);
        println!(
            "Starting at index {first_no_label} with label {:?}",
            data[first_no_label]
        );

        Ok(Dataset {
            data,
            i: first_no_label,
        })
    }
    pub fn with_label_prefix(prefix: &str) -> Result<Self> {
        let mut dataset = Dataset::from_input_dir()?;
        for mut datapoint in &mut dataset.data {
            let label_name = datapoint.label_src.file_name().unwrap().to_str().unwrap();
            let label_prefix_name = format!("{prefix}{label_name}");
            datapoint.label_src = datapoint.label_src.with_file_name(label_prefix_name);
        }
        Ok(dataset)
    }

    pub fn current_image(&self) -> Result<ColorImage> {
        self.data[self.i].load_image()
    }
    pub fn current_label(&self) -> Result<YoloLabel<L>> {
        self.data[self.i].load_label()
    }
    pub fn previous_label(&self) -> Result<YoloLabel<L>> {
        let previous = self.i.saturating_sub(1);
        self.data[previous].load_label()
    }
    pub fn current_name(&self) -> String {
        self.data[self.i].name()
    }
    pub fn get_progress(&self) -> (usize, usize, usize) {
        (0, self.i, self.data.len())
    }
    fn save_label(&self, label: YoloLabel<L>) -> Result<()> {
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
    fn next_containing(&mut self, classes: &HashSet<L>) -> Result<()> {
        while self.i < self.data.len() - 1 {
            self.i += 1;
            let label = self.data[self.i].load_label()?;
            if label.iter().any(|bb| classes.contains(&bb.class())) {
                break;
            }
        }
        Ok(())
    }
    fn previous_containing(&mut self, classes: &HashSet<L>) -> Result<()> {
        while self.i > 0 {
            self.i -= 1;
            let label = self.data[self.i].load_label()?;
            if label.iter().any(|bb| classes.contains(&bb.class())) {
                break;
            }
        }
        Ok(())
    }
    fn go_to(&mut self, pos: usize) -> Result<()> {
        self.i = pos.clamp(0, self.data.len() - 1);
        Ok(())
    }

    pub fn go(&mut self, movement: DatasetMovement<L>, label: YoloLabel<L>) -> Result<()> {
        self.save_label(label)?;
        match movement {
            DatasetMovement::Next => self.next(),
            DatasetMovement::Previous => self.previous(),
            DatasetMovement::NextContaining(classes) => self.next_containing(&classes),
            DatasetMovement::PreviousContaining(classes) => self.previous_containing(&classes),
            DatasetMovement::JumpTo(pos) => self.go_to(pos),
        }
    }
}
