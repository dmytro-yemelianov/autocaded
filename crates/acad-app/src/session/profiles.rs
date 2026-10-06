//! Profile application only changes the presentation preset and current palette.
use super::Session;
use acad_cmd::{
    messages::text,
    profiles::{ProfileId, PROFILES},
};

impl Session {
    pub fn profile(&self) -> ProfileId {
        self.profile
    }
    pub fn apply_profile(&mut self, profile: ProfileId) {
        self.profile = profile;
        self.set_palette(profile.definition().palette);
    }
    /// Parse before mutation: malformed/unknown IDs preserve all session state.
    pub fn set_mode(&mut self, id: &str) -> Result<(), String> {
        let profile = ProfileId::parse(id)?;
        self.apply_profile(profile);
        Ok(())
    }
    /// Resolved labels and presets, with current palette kept separate from ID.
    pub fn presentation_profiles(&self) -> serde_json::Value {
        let profiles: Vec<_> = PROFILES
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.stable_id, "palette": p.palette.name(), "tone": p.tone.css_name(),
                    "title": text(p.title, self.locale).expect("profile label"),
                    "badge": text(p.badge, self.locale).expect("profile label"),
                    "status": text(p.status, self.locale).expect("profile label"),
                    "source": p.source
                })
            })
            .collect();
        serde_json::json!({"schema_version":1, "mode":self.profile.definition().stable_id,
            "palette": self.palette().name(), "profiles": profiles})
    }
}
