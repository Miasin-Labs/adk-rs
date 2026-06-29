use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

#[derive(Debug, Default, Clone)]
pub struct SkillRegistry {
    skills: BTreeMap<String, Skill>,
}

impl SkillRegistry {
    pub fn register(&mut self, skill: Skill) {
        self.skills.insert(skill.name.clone(), skill);
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }
}
