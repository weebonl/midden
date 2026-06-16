use crate::{
    config::{ActionRule, RuntimeSettings},
    db::{Role, User},
};

pub fn allowed(rule: ActionRule, user: Option<&User>) -> bool {
    match rule {
        ActionRule::Disabled => false,
        ActionRule::Anonymous => true,
        ActionRule::Authenticated => user.is_some(),
        ActionRule::Moderator => user.is_some_and(|user| user.role >= Role::Moderator),
        ActionRule::Admin => user.is_some_and(|user| user.role >= Role::Admin),
        ActionRule::Owner => user.is_some_and(|user| user.role >= Role::Owner),
    }
}

pub fn can_upload_file(settings: &RuntimeSettings, user: Option<&User>) -> bool {
    settings.features.files && allowed(settings.policy.upload_file, user)
}

pub fn can_create_paste(settings: &RuntimeSettings, user: Option<&User>) -> bool {
    settings.features.pastes && allowed(settings.policy.create_paste, user)
}

pub fn can_use_api(settings: &RuntimeSettings, user: Option<&User>) -> bool {
    settings.features.api && allowed(settings.policy.use_api, user)
}

pub fn can_admin(user: Option<&User>) -> bool {
    user.is_some_and(|user| user.role >= Role::Admin)
}

pub fn can_moderate(user: Option<&User>) -> bool {
    user.is_some_and(|user| user.role >= Role::Moderator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_rule_allows_no_user() {
        assert!(allowed(ActionRule::Anonymous, None));
        assert!(!allowed(ActionRule::Authenticated, None));
    }
}
