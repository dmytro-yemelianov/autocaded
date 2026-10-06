//! Shared presentation presets. Profile identity and current palette are independent.
use crate::messages::MessageId;

#[derive(Clone, Copy, Debug)]
pub enum BadgeTone {
    Green,
    Red,
}
impl BadgeTone {
    pub fn css_name(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Red => "red",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ProfileDef {
    pub id: ProfileId,
    pub stable_id: &'static str,
    pub palette: acad_model::Palette,
    pub tone: BadgeTone,
    pub title: MessageId,
    pub badge: MessageId,
    pub status: MessageId,
    pub source: &'static str,
}
include!(concat!(env!("OUT_DIR"), "/profiles.rs"));
pub const CATALOG_JSON: &str = include_str!("../resources/profiles.json");
impl ProfileId {
    pub fn parse(id: &str) -> Result<Self, String> {
        PROFILES
            .iter()
            .find(|p| p.stable_id == id)
            .map(|p| p.id)
            .ok_or_else(|| format!("unknown profile '{id}' (frozen or modern)"))
    }
    pub fn definition(self) -> &'static ProfileDef {
        PROFILES
            .iter()
            .find(|p| p.id == self)
            .expect("generated profile identity")
    }
}
