use anyhow::Context;
use sqlx::postgres::PgConnection;

use crate::{
    util::{extension_versions, update_paths},
    version_graph::VersionGraph,
};

// `create extension` takes no bind parameters, so names and versions are
// interpolated. Double the quote characters so an embedded quote can't end the
// identifier or literal early.
fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub async fn create(
    mut conn: PgConnection,
    extension_name: &str,
    schema: Option<&str>,
    version: Option<&str>,
    cascade: bool,
) -> anyhow::Result<()> {
    // pg_tle can only create a version that ships a base install script. When
    // the requested version exists solely as an upgrade target, create the
    // nearest base and upgrade into it instead of failing with
    // `could not find sql function "<ext>--<version>.sql"` (#159).
    let upgrade_target = match version {
        Some(target) => resolve_upgrade_target(&mut conn, extension_name, target).await?,
        None => None,
    };

    let create_version = match &upgrade_target {
        Some(route) => Some(route.base.as_str()),
        None => version,
    };

    let mut query = format!(
        "create extension if not exists {}",
        quote_ident(extension_name)
    );

    if let Some(schema_name) = schema {
        query.push_str(&format!(" schema {}", quote_ident(schema_name)));
    }

    if let Some(ver) = create_version {
        query.push_str(&format!(" version {}", quote_literal(ver)));
    }

    if cascade {
        query.push_str(" cascade");
    }

    query.push(';');

    sqlx::query(&query)
        .execute(&mut conn)
        .await
        .context(format!("failed to create extension {}", extension_name))?;

    match upgrade_target {
        Some(route) => {
            println!(
                "Extension \"{}\" created at version {} (no base install script for {})",
                extension_name, route.base, route.target
            );

            let update = format!(
                "alter extension {} update to {};",
                quote_ident(extension_name),
                quote_literal(&route.target)
            );

            sqlx::query(&update)
                .execute(&mut conn)
                .await
                .context(format!(
                    "failed to upgrade extension {} from {} to {}",
                    extension_name, route.base, route.target
                ))?;

            println!("Upgraded \"{}\" to version {}", extension_name, route.target);
        }
        None => println!("Extension \"{}\" created successfully", extension_name),
    }

    Ok(())
}

struct UpgradeRoute {
    base: String,
    target: String,
}

/// `None` when `target` can be created directly, which is the common case.
async fn resolve_upgrade_target(
    conn: &mut PgConnection,
    extension_name: &str,
    target: &str,
) -> anyhow::Result<Option<UpgradeRoute>> {
    let bases = extension_versions(conn, extension_name).await?;
    let paths = update_paths(conn, extension_name).await?;

    let graph = VersionGraph::new(
        bases,
        paths.into_iter().map(|path| (path.source, path.target)),
    );

    if graph.is_base(target) {
        return Ok(None);
    }

    match graph.base_for(target) {
        Some(base) => Ok(Some(UpgradeRoute {
            base,
            target: target.to_string(),
        })),
        // Leave it to Postgres to raise: it reports the missing script far more
        // precisely than a guess here would.
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_are_doubled_not_dropped() {
        assert_eq!(quote_ident(r#"ext"; drop table t; --"#), r#""ext""; drop table t; --""#);
        assert_eq!(quote_literal("1.0'; drop table t; --"), "'1.0''; drop table t; --'");
    }
}
