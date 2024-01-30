pub mod conflicts;
pub mod dataset;
pub mod file_loader;
pub mod labeling;
pub mod relabeling;
pub mod tagging;

pub mod check;
pub mod shady;

// pub use conflicts::Conflicts;
use dataset::{Datapoint, Dataset};
use egui::{Response, Ui};
pub use labeling::Labeling;
pub use relabeling::Relabeling;
pub use tagging::Tagging;

pub struct SyncDatasets {
    pub current_pos: usize,
}

pub trait Tool {
    fn draw_ui(&mut self, ui: &mut Ui) -> anyhow::Result<()>;
    // fn try_load(ctx: &Context) -> anyhow::Result<Self>
    // where
    //     Self: Sized;
    fn draw_in_central_panel(
        &mut self,
        central_panel: &mut Ui,
        img_response: Response,
        datset: &mut Dataset,
    ) -> anyhow::Result<()>;
    fn refresh_state(&mut self, datapoint: &Datapoint) -> anyhow::Result<()>;
    fn save_state(&self, datapoint: &Datapoint) -> anyhow::Result<()>;
    fn name(&self) -> &str;
}
