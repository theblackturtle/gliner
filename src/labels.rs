//! Label definitions replicated from the Python implementation.

pub const PII_LABELS: &[&str] = &[
    "person",
    "email",
    "phone",
    "address",
    "credit card",
    "ssn",
    "account number",
    "password",
    "username",
    "ip address",
    "date",
    "location",
    "organization",
];

pub const EXTENDED_PII_LABELS: &[&str] = &[
    "person",
    "name",
    "first name",
    "last name",
    "dob",
    "age",
    "gender",
    "email",
    "email address",
    "phone",
    "phone number",
    "ip address",
    "url",
    "address",
    "location",
    "street",
    "city",
    "state",
    "country",
    "zip",
    "account number",
    "bank account",
    "routing number",
    "credit card",
    "card number",
    "cvv",
    "ssn",
    "money",
    "condition",
    "medical",
    "drug",
    "medication",
    "blood type",
    "passport",
    "driver license",
    "username",
    "password",
    "license plate",
    "organization",
    "company",
];

/// Labels optimised for detecting sensitive conversational content.
pub const PRIVATE_CONVERSATION_LABELS: &[&str] = &[
    "conversation",
    "private conversation",
    "direct message",
    "chat message",
    "im message",
    "slack message",
    "zoom chat",
    "meet transcript",
    "speaker",
    "listener",
    "confidential discussion",
    "customer message",
    "agent message",
];

/// Labels useful for flagging sensitive logging output.
pub const LOG_DATA_LABELS: &[&str] = &[
    "log entry",
    "stack trace",
    "exception",
    "error message",
    "debug message",
    "credential",
    "token",
    "api key",
    "secret",
    "session id",
    "container id",
    "hostname",
    "ip address",
];

/// Named label packs exposed through the CLI and server configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelProfile {
    Default,
    Extended,
    PrivateConversation,
    LogIntelligence,
}

impl LabelProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelProfile::Default => "default",
            LabelProfile::Extended => "extended",
            LabelProfile::PrivateConversation => "private_conversation",
            LabelProfile::LogIntelligence => "log_intelligence",
        }
    }

    pub fn labels(self) -> &'static [&'static str] {
        match self {
            LabelProfile::Default => PII_LABELS,
            LabelProfile::Extended => EXTENDED_PII_LABELS,
            LabelProfile::PrivateConversation => PRIVATE_CONVERSATION_LABELS,
            LabelProfile::LogIntelligence => LOG_DATA_LABELS,
        }
    }
}

pub fn profile_from_str(name: &str) -> Option<LabelProfile> {
    let lowered = name.trim().to_lowercase();
    match lowered.as_str() {
        "default" => Some(LabelProfile::Default),
        "extended" => Some(LabelProfile::Extended),
        "private_conversation" | "conversation" | "private" => {
            Some(LabelProfile::PrivateConversation)
        }
        "log_intelligence" | "logs" | "log" => Some(LabelProfile::LogIntelligence),
        _ => None,
    }
}

pub const TEXT_EXTENSIONS: &[&str] = &[
    ".txt", ".json", ".xml", ".csv", ".log", ".md", ".py", ".js", ".java", ".cpp", ".c", ".h",
    ".html", ".css", ".yaml", ".yml", ".ini", ".conf", ".config", ".sql", ".sh", ".bash", ".env",
];
