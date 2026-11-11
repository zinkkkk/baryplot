//! Messages from user input and workers, plus navigation and field identifiers.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Nodes,
    Definitions,
    Fit,
    Spectrum,
    Plot,
    Path,
    Help,
}
#[derive(Debug, Clone, Copy)]
pub(super) enum Pane {
    Editor,
    Plot,
}
#[derive(Debug, Clone, Copy)]
pub enum Flag {
    Output,
    InputPath,
    WeightedValues,
    Grid,
    UnitCircle,
    Nodes,
    Values,
    Weights,
    Coloring,
    Poles,
    Zeros,
    Residues,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Samples,
    Start,
    End,
    Extrapolation,
    GridStep,
    MarginX,
    MarginY,
}

#[derive(Debug, Clone, Copy)]
pub enum GlobalField {
    Name,
    Parameters,
    Expression,
}

#[derive(Debug, Clone)]
pub enum Message {
    Appearance(appearance::Action),
    HoverEntry(usize, bool),
    FrontEntry(usize),
    BeginEntryDrag(usize),
    EndEntryDrag,
    ReorderEntry(usize, usize),
    ScrollEntries(f32),
    Escape,
    Edit(usize, usize, String),
    EditImaginary(usize, usize, String),
    Name(usize, String),
    Enabled(usize, bool),
    Visible(usize, bool),
    Lock(usize, usize, usize, bool),
    Add,
    Remove(usize),
    ClearEntries,
    GlobalKind(usize, model::DefinitionKind),
    GlobalEdit(usize, GlobalField, String),
    EditNumeric(usize, iced::widget::text_editor::Action),
    AddGlobal,
    RemoveGlobal(usize),
    Mode(Mode),
    Tab(Tab),
    ToggleEditor,
    NavigateFocus(bool),
    PlotSize(iced::Size),
    Undo,
    Redo,
    Field(Field, String),
    Axis(usize, String),
    Flag(Flag, bool),
    LineOpacity(f32),
    ColorOpacity(f32),
    Extent(Extent),
    Curve(Curve),
    SelectPath(usize),
    TogglePath(usize),
    EditPath(usize, Box<Message>),
    AddPath(model::InputKind),
    RemovePath,
    PathName(String),
    PathEnabled(bool),
    PathKind(model::InputKind),
    ControlWeight(usize, String),
    ControlAxis(usize, usize, String),
    ResetPath,
    AddControl,
    ClearPath,
    DrawPath(bool),
    ResetView,
    Plot(Interaction),
    Resize(pane_grid::ResizeEvent),
    Rendered(RenderResult),
    ImageReady(
        TextureUpload,
        Result<iced::widget::image::Allocation, iced::widget::image::Error>,
    ),
    Spectrum(crate::numerics::filter::Action),
    Fit(FitAction),
    CancelSpectrum,
    CalculateSpectrum,
    FitProgress(u64, crate::numerics::aaa::Progress),
    Fitted(u64, Result<Option<crate::numerics::aaa::Fit>, String>),
    Analyzed(u64, Result<Option<crate::numerics::Spectrum>, String>),
    ProjectPath(String),
    Save,
    Load,
    Preset(Mode),
}
