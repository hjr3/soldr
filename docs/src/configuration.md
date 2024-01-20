# Configuration

Soldr is designed to start running without requiring a configuration file.

You can configure soldr by creating a configuration file in the [TOML](https://toml.io) format. 

Example:

```toml
database_url = "sqlite:soldr.db?mode=rwc"
management_listener = "0.0.0.0:3443"
ingest_listener = "0.0.0.0:3000"
```

The repository also includes `soldr.example.toml` for reference.

- `database_url` - the SQLite database connection string
- `management_listener` - the ip address and port for the management API
- `ingest_listener` - the ip address and port for the proxy
