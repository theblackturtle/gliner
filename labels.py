"""
PII Label Definitions for GLiNER Model

This module defines different sets of PII labels optimized for various use cases.
"""

# Optimized PII labels for general NER models (fewer labels = faster)
# GLiNER v2.1 works best with simpler, more common entity types
PII_LABELS = [
    # Core personal info
    "person",
    "email",
    "phone",
    "address",
    # Financial
    "credit card",
    "ssn",
    "account number",
    # Identifiers
    "password",
    "username",
    "ip address",
    # Dates and locations
    "date",
    "location",
    "organization",
]

# Extended labels for more thorough scanning
# Use this for comprehensive PII detection (slower but more thorough)
EXTENDED_PII_LABELS = [
    # Personal Identifiers
    "person",
    "name",
    "first name",
    "last name",
    "dob",
    "age",
    "gender",
    # Contact Information
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
    # Financial Information
    "account number",
    "bank account",
    "routing number",
    "credit card",
    "card number",
    "cvv",
    "ssn",
    "money",
    # Healthcare Information
    "condition",
    "medical",
    "drug",
    "medication",
    "blood type",
    # Identification Documents
    "passport",
    "driver license",
    "username",
    "password",
    "license plate",
    # Organizations
    "organization",
    "company",
]

# Text file extensions to scan
TEXT_EXTENSIONS = {
    ".txt",
    ".json",
    ".xml",
    ".csv",
    ".log",
    ".md",
    ".py",
    ".js",
    ".java",
    ".cpp",
    ".c",
    ".h",
    ".html",
    ".css",
    ".yaml",
    ".yml",
    ".ini",
    ".conf",
    ".config",
    ".sql",
    ".sh",
    ".bash",
    ".env",
}

