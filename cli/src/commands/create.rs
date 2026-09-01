use anyhow::Context;
use sqlx::postgres::PgConnection;

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
    let mut query = format!(
        "create extension if not exists {}",
        quote_ident(extension_name)
    );

    if let Some(schema_name) = schema {
        query.push_str(&format!(" schema {}", quote_ident(schema_name)));
    }

    if let Some(ver) = version {
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

    println!("Extension \"{}\" created successfully", extension_name);

    Ok(())
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
