use std::collections::HashSet;

use crate::{
    models::{Payload, UpdatePath},
    util::{extension_versions, update_paths},
    version_graph::VersionGraph,
};
use anyhow::Context;
use sqlx::postgres::PgConnection;

pub async fn install(payload: &Payload, mut conn: PgConnection) -> anyhow::Result<()> {
    let existing_versions = extension_versions(&mut conn, &payload.metadata.extension_name).await?;
    let mut versions_installed_now = HashSet::new();

    let mut installed_extension_once = !existing_versions.is_empty();

    // Only the default version's own lineage is worth installing. Versions on a
    // branch that cannot reach it would sit in the catalog implying an upgrade
    // path that was never published (#387).
    let graph = VersionGraph::from_payload(payload);
    let required = graph.required_for(&payload.metadata.default_version);

    for install_file in &payload.install_files {
        if !required.contains(&install_file.version) {
            continue;
        }
        if !existing_versions.contains(&install_file.version) {
            if installed_extension_once {
                sqlx::query("select pgtle.install_extension_version_sql($1, $2, $3)")
                    .bind(&payload.metadata.extension_name)
                    .bind(&install_file.version)
                    .bind(&install_file.body)
                    .execute(&mut conn)
                    .await
                    .context(format!(
                        "failed to install extension version {}",
                        install_file.filename
                    ))?;
                println!("Installed version {}", install_file.version);
                versions_installed_now.insert(install_file.version.clone());
            } else {
                sqlx::query("select pgtle.install_extension($1, $2, $3, $4, $5)")
                    .bind(&payload.metadata.extension_name)
                    .bind(&install_file.version)
                    .bind(&payload.metadata.comment)
                    .bind(&install_file.body)
                    .bind(&payload.metadata.requires)
                    .execute(&mut conn)
                    .await
                    .context(format!(
                        "failed to install extension {}",
                        install_file.filename
                    ))?;
                println!("Installed version {}", install_file.version);
                versions_installed_now.insert(install_file.version.clone());
                installed_extension_once = true;
            }
        }
    }

    let existing_update_paths = update_paths(&mut conn, &payload.metadata.extension_name).await?;

    for upgrade_file in &payload.upgrade_files {
        if !VersionGraph::edge_is_required(
            &required,
            &upgrade_file.from_version,
            &upgrade_file.to_version,
        ) {
            continue;
        }
        if !existing_update_paths.contains(&UpdatePath {
            source: upgrade_file.from_version.clone(),
            target: upgrade_file.to_version.clone(),
        }) {
            sqlx::query("select pgtle.install_update_path($1, $2, $3, $4)")
                .bind(&payload.metadata.extension_name)
                .bind(&upgrade_file.from_version)
                .bind(&upgrade_file.to_version)
                .bind(&upgrade_file.body)
                .execute(&mut conn)
                .await
                .context(format!(
                    "failed to install update path {}",
                    upgrade_file.filename
                ))?;
            println!(
                "Installed update file from version {} to {}",
                upgrade_file.from_version, upgrade_file.to_version
            );
        }
    }

    sqlx::query("select pgtle.set_default_version($1, $2)")
        .bind(&payload.metadata.extension_name)
        .bind(&payload.metadata.default_version)
        .execute(&mut conn)
        .await
        .context(format!(
            "failed to set default version to {}",
            &payload.metadata.default_version
        ))?;

    if !versions_installed_now.contains(&payload.metadata.default_version) {
        println!(
            "Set default version to {}",
            payload.metadata.default_version
        );
    }

    Ok(())
}
