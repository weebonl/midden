use crate::{
    app::{AppError, AppResult},
    config::{RuntimeSettings, ScanningConfig},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Paste,
}

impl ItemKind {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "file" => Ok(Self::File),
            "paste" => Ok(Self::Paste),
            _ => Err(AppError::NotFound),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Paste => "paste",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Active,
    Quarantined,
    Takedown,
    LegalHold,
    Deleted,
}

impl ItemState {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "active" => Ok(Self::Active),
            "quarantined" => Ok(Self::Quarantined),
            "takedown" => Ok(Self::Takedown),
            "legal_hold" => Ok(Self::LegalHold),
            "deleted" => Ok(Self::Deleted),
            _ => Err(AppError::BadRequest("invalid item state".to_string())),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Quarantined => "quarantined",
            Self::Takedown => "takedown",
            Self::LegalHold => "legal_hold",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemVisibility {
    Unlisted,
    Public,
    Private,
}

impl ItemVisibility {
    pub fn parse(settings: &RuntimeSettings, value: &str) -> AppResult<Self> {
        match value.trim() {
            "unlisted" => Ok(Self::Unlisted),
            "public" if settings.features.public_browse => Ok(Self::Public),
            "public" => Err(AppError::BadRequest(
                "public visibility requires public browse to be enabled".to_string(),
            )),
            "private" => Ok(Self::Private),
            _ => Err(AppError::BadRequest("invalid visibility".to_string())),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unlisted => "unlisted",
            Self::Public => "public",
            Self::Private => "private",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportAction {
    Resolve,
    Dismiss,
    Quarantine,
    Takedown,
    LegalHold,
}

impl ReportAction {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "resolve" => Ok(Self::Resolve),
            "dismiss" => Ok(Self::Dismiss),
            "quarantine" => Ok(Self::Quarantine),
            "takedown" => Ok(Self::Takedown),
            "legal_hold" => Ok(Self::LegalHold),
            _ => Err(AppError::BadRequest("unknown report action".to_string())),
        }
    }

    pub const fn report_state(self) -> &'static str {
        match self {
            Self::Dismiss => "dismissed",
            Self::Resolve | Self::Quarantine | Self::Takedown | Self::LegalHold => "resolved",
        }
    }

    pub const fn item_state(self) -> Option<ItemState> {
        match self {
            Self::Resolve | Self::Dismiss => None,
            Self::Quarantine => Some(ItemState::Quarantined),
            Self::Takedown => Some(ItemState::Takedown),
            Self::LegalHold => Some(ItemState::LegalHold),
        }
    }
}

#[derive(Debug)]
pub struct ItemModerationPlan {
    pub kind: ItemKind,
    pub public_id: String,
    pub state: Option<ItemState>,
    pub visibility: Option<ItemVisibility>,
    pub note: Option<String>,
    pub block_hash: bool,
    pub(crate) scanning_fallback: Option<ScanningConfig>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ItemModerationOutcome {
    Applied { zero_ref_blob_hash: Option<String> },
    NotFound,
    TerminalFileTransition,
}

#[derive(Debug, Clone, Copy)]
pub enum AccountBulkAction {
    Delete,
    SetVisibility(ItemVisibility),
    SetExpiry {
        file_expires_at: Option<i64>,
        paste_expires_at: Option<i64>,
    },
}

#[derive(Debug)]
pub struct AccountBulkPlan {
    pub owner_user_id: String,
    pub file_ids: Vec<String>,
    pub paste_ids: Vec<String>,
    pub action: AccountBulkAction,
    pub allow_delete_any_owner: bool,
}

#[derive(Debug, Default)]
pub struct AccountBulkResult {
    pub zero_ref_blob_hashes: Vec<String>,
}

impl ItemModerationPlan {
    pub fn new(kind: ItemKind, public_id: String) -> Self {
        Self {
            kind,
            public_id,
            state: None,
            visibility: None,
            note: None,
            block_hash: false,
            scanning_fallback: None,
        }
    }

    pub(crate) fn has_mutation(&self) -> bool {
        self.state.is_some() || self.visibility.is_some() || self.note.is_some() || self.block_hash
    }
}
