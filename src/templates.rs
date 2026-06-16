use std::{path::Path, sync::Arc};

use minijinja::{AutoEscape, Environment, context};

use crate::config::{AppConfig, RuntimeSettings};

#[derive(Clone)]
pub struct Templates {
    env: Arc<Environment<'static>>,
}

impl Templates {
    pub fn load(config: &AppConfig) -> anyhow::Result<Self> {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|name| {
            if name.ends_with(".html") {
                AutoEscape::Html
            } else {
                AutoEscape::None
            }
        });
        env.add_function("human_bytes", crate::util::human_bytes);

        for (name, source) in BUILTIN_TEMPLATES {
            let source = override_or_builtin(config.server.template_dir.as_deref(), name, source)?;
            env.add_template_owned(name.to_string(), source)?;
        }

        Ok(Self { env: Arc::new(env) })
    }

    pub fn render<S: serde::Serialize>(
        &self,
        name: &str,
        settings: &RuntimeSettings,
        current_user: Option<&crate::db::User>,
        value: S,
    ) -> anyhow::Result<String> {
        let template = self.env.get_template(name)?;
        Ok(template.render(context! {
            settings => settings,
            current_user => current_user,
            page => value,
        })?)
    }
}

fn override_or_builtin(
    template_dir: Option<&Path>,
    name: &str,
    builtin: &str,
) -> anyhow::Result<String> {
    if let Some(template_dir) = template_dir {
        let path = template_dir.join(name);
        if path.exists() {
            return Ok(std::fs::read_to_string(path)?);
        }
    }
    Ok(builtin.to_string())
}

const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    ("base.html", include_str!("../templates/base.html")),
    ("index.html", include_str!("../templates/index.html")),
    ("browse.html", include_str!("../templates/browse.html")),
    (
        "upload_result.html",
        include_str!("../templates/upload_result.html"),
    ),
    (
        "resumable_upload.html",
        include_str!("../templates/resumable_upload.html"),
    ),
    (
        "url_upload.html",
        include_str!("../templates/url_upload.html"),
    ),
    (
        "paste_new.html",
        include_str!("../templates/paste_new.html"),
    ),
    (
        "paste_show.html",
        include_str!("../templates/paste_show.html"),
    ),
    (
        "paste_edit.html",
        include_str!("../templates/paste_edit.html"),
    ),
    (
        "paste_result.html",
        include_str!("../templates/paste_result.html"),
    ),
    (
        "file_preview.html",
        include_str!("../templates/file_preview.html"),
    ),
    ("takedown.html", include_str!("../templates/takedown.html")),
    ("login.html", include_str!("../templates/login.html")),
    (
        "password_reset_request.html",
        include_str!("../templates/password_reset_request.html"),
    ),
    (
        "password_reset_form.html",
        include_str!("../templates/password_reset_form.html"),
    ),
    (
        "email_verified.html",
        include_str!("../templates/email_verified.html"),
    ),
    (
        "two_factor.html",
        include_str!("../templates/two_factor.html"),
    ),
    ("register.html", include_str!("../templates/register.html")),
    ("account.html", include_str!("../templates/account.html")),
    ("admin.html", include_str!("../templates/admin.html")),
    (
        "admin_search.html",
        include_str!("../templates/admin_search.html"),
    ),
    (
        "admin_users.html",
        include_str!("../templates/admin_users.html"),
    ),
    ("reports.html", include_str!("../templates/reports.html")),
    (
        "admin_item.html",
        include_str!("../templates/admin_item.html"),
    ),
    (
        "report_form.html",
        include_str!("../templates/report_form.html"),
    ),
    (
        "delete_form.html",
        include_str!("../templates/delete_form.html"),
    ),
    (
        "delete_result.html",
        include_str!("../templates/delete_result.html"),
    ),
    (
        "claim_form.html",
        include_str!("../templates/claim_form.html"),
    ),
    ("error.html", include_str!("../templates/error.html")),
    ("docs.html", include_str!("../templates/docs.html")),
];
