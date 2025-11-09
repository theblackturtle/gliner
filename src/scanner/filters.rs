use std::collections::{HashMap, HashSet};

use regex::Regex;

#[derive(Debug, Clone)]
pub struct FilterRule {
    pub exact_match: HashSet<String>,
    pub patterns: Vec<Regex>,
    pub valid_pattern: Option<Regex>,
    pub min_confidence: Option<f32>,
}

impl FilterRule {
    pub fn is_false_positive(&self, value: &str, score: f32) -> bool {
        if let Some(min_confidence) = self.min_confidence {
            if score < min_confidence {
                return true;
            }
        }

        let text_lower = value.trim().to_lowercase();

        if self.exact_match.contains(&text_lower) {
            return true;
        }

        for regex in &self.patterns {
            if regex.is_match(&text_lower) {
                return true;
            }
        }

        if let Some(valid_regex) = &self.valid_pattern {
            if !valid_regex.is_match(value) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
pub struct FalsePositiveFilters {
    rules: HashMap<String, FilterRule>,
}

impl FalsePositiveFilters {
    pub fn new() -> Self {
        Self {
            rules: build_default_rules(),
        }
    }

    pub fn is_false_positive(&self, label: &str, text: &str, score: f32) -> bool {
        if let Some(rule) = self.rules.get(&label.to_lowercase()) {
            return rule.is_false_positive(text, score);
        }
        false
    }
}

fn build_default_rules() -> HashMap<String, FilterRule> {
    let mut rules = HashMap::new();

    rules.insert(
        "password".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                ["password", "pwd", "passwd", "pass"].map(str::to_string),
            ),
            patterns: vec![
                regex(r"^password$"),
                regex(r"^pwd$"),
                regex(r"password\d*$"),
            ],
            valid_pattern: None,
            min_confidence: None,
        },
    );

    rules.insert(
        "email".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                ["email", "emailaddress", "useremail", "e-mail"].map(str::to_string),
            ),
            patterns: vec![regex(r"^email$"), regex(r"^.*email.*$")],
            valid_pattern: Some(regex(r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$")),
            min_confidence: None,
        },
    );

    rules.insert(
        "phone".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                ["phone", "phonenumber", "mobile", "tel", "telephone"].map(str::to_string),
            ),
            patterns: vec![
                regex(r"^iphone\d+$"),
                regex(r"^phone$"),
                regex(r".*phone.*"),
            ],
            valid_pattern: Some(regex(r".*\d.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*")),
            min_confidence: None,
        },
    );

    rules.insert(
        "address".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                [
                    "address",
                    "addr",
                    "location",
                    "phonenumber",
                    "emailaddress",
                    "emailaddressvalue",
                ]
                .map(str::to_string),
            ),
            patterns: vec![regex(r"^address$"), regex(r"^.*address.*$")],
            valid_pattern: None,
            min_confidence: None,
        },
    );

    rules.insert(
        "credit card".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                [
                    "card",
                    "creditcard",
                    "visa",
                    "mastercard",
                    "amex",
                    "discover",
                ]
                .map(str::to_string),
            ),
            patterns: vec![
                regex(r"^.*card.*$"),
                regex(r"^visa$"),
                regex(r"^mastercard$"),
            ],
            valid_pattern: Some(regex(r"^\d{13,19}$")),
            min_confidence: None,
        },
    );

    rules.insert(
        "ssn".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                ["ssn", "socialsecurity", "social"].map(str::to_string),
            ),
            patterns: vec![regex(r"^[a-z]{1,5}$"), regex(r"^[A-Z][a-z]+$")],
            valid_pattern: Some(regex(r"^\d{3}-?\d{2}-?\d{4}$")),
            min_confidence: None,
        },
    );

    rules.insert(
        "account number".to_string(),
        FilterRule {
            exact_match: HashSet::new(),
            patterns: vec![regex(
                r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
            )],
            valid_pattern: None,
            min_confidence: Some(0.6),
        },
    );

    rules.insert(
        "ip address".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(["prod", "staging", "dev"].map(str::to_string)),
            patterns: vec![
                regex(r"^[a-z]{2,3}-[a-z]+-\d+$"),
                regex(r"^prod-.*$"),
                regex(r"^staging-.*$"),
                regex(r"^dev-.*$"),
            ],
            valid_pattern: Some(regex(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$")),
            min_confidence: None,
        },
    );

    rules.insert(
        "username".to_string(),
        FilterRule {
            exact_match: HashSet::from_iter(
                ["username", "user", "userid", "user_id"].map(str::to_string),
            ),
            patterns: vec![regex(r"^.*uuid$"), regex(r"^.*_uuid$"), regex(r"^.*Uuid$")],
            valid_pattern: None,
            min_confidence: None,
        },
    );

    rules.insert(
        "date".to_string(),
        FilterRule {
            exact_match: HashSet::new(),
            patterns: Vec::new(),
            valid_pattern: None,
            min_confidence: Some(0.7),
        },
    );

    rules
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|err| panic!("Invalid regex {}: {}", pattern, err))
}
