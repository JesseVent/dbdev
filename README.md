![dbdev](/assets/dbdev-banner.jpg)

# dbdev

dbdev is a package manager for Postgres [trusted language extensions (TLE)](https://github.com/aws/pg_tle).

## Links

- Search for packages on [database.dev](https://database.dev)
- Documentation: [database.dev/docs](https://database.dev/docs)
- Publish your own Extension: [database.dev/docs/publish-extension](https://database.dev/docs/publish-extension)
- Read the dbdev [release blog post](https://supabase.com/blog/dbdev)

## What is a Trusted Language Extension?

Trusted Language Extensions (TLE) allow PostgreSQL extensions to be installed by non-superusers by restricting them to trusted languages such as SQL and PL/pgSQL. Because these languages cannot access the operating system or unsafe memory, the extensions can run safely in managed environments where superuser access is not available. TLEs are commonly used in hosted PostgreSQL services to enable a safer extension ecosystem without granting elevated privileges.

## How does dbdev compare to PGXN?

[PGXN](https://pgxn.org) distributes traditional PostgreSQL extensions, including C ones. Installing from PGXN generally needs a compiler toolchain, shell access to the host, and filesystem privileges to place `.so` files in Postgres's library directory.

dbdev distributes trusted language extensions instead. Packages are SQL or a trusted procedural language, so they load over a normal database connection with no compilation, no shell, and no superuser. That is what lets managed services such as Supabase and Amazon RDS install community extensions safely.

The two are complementary: PGXN if your extension needs C, dbdev if it can be expressed in a trusted language. See the [FAQ](https://database.dev/docs/faq) for a fuller comparison.

## Licence

Apache 2.0
