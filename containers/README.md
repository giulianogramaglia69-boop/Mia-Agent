# Container MCP (Fase 4)

Ogni capacità del mondo reale vive in un container e parla MCP su
stdio. Il client è il binario `mia`, che li lancia e ne espone i tool
al runtime (vedi `mia-data/mcp.yaml`).

| Container | Server | Tool | Scritture |
| --- | --- | --- | --- |
| `mia-notes` | note in file di testo | `list-notes`, `read-note`, `write-note`, `delete-note` | `write-note` idempotente, `delete-note` distruttivo |
| `mia-browser` | lettore web (solo http/https) | `fetch-url` | nessuna (sola lettura) |
| `mia-calendar` | Google Calendar | `list-events`, `create-event`, `google-auth-status` | `create-event` |
| `mia-mail` | Gmail | `list-messages`, `read-message`, `send-message`, `google-auth-status` | `send-message` (serve conferma) |

## Uso

```sh
docker compose build
docker compose run --rm calendar auth   # autorizza Google Calendar (una volta)
docker compose run --rm mail auth       # autorizza Gmail (una volta)
```

Le credenziali OAuth (Google Cloud, client "Desktop") vanno in un
file `.env` accanto a `docker-compose.yml` (mai nel repo):

```
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
```

`mcp.yaml` (in `mia-data/`) per usare i container da Mia:

```yaml
servers:
  - name: notes
    transport: stdio
    program: docker
    args: ["compose", "run", "--rm", "-i", "notes"]
    read_only_tools: [list-notes, read-note]
    write_policy: allow
  - name: browser
    transport: stdio
    program: docker
    args: ["compose", "run", "--rm", "-i", "browser"]
    read_only_tools: [fetch-url]
    write_policy: deny
  - name: calendar
    transport: stdio
    program: docker
    args: ["compose", "run", "--rm", "-i", "calendar"]
    read_only_tools: [list-events, google-auth-status]
    write_policy: prompt
  - name: mail
    transport: stdio
    program: docker
    args: ["compose", "run", "--rm", "-i", "mail"]
    read_only_tools: [list-messages, read-message, google-auth-status]
    write_policy: prompt
```

## Sviluppo locale (senza Docker)

Ogni server è anche un binario del workspace e gira diretto su stdio:

```sh
cargo run -p mia-notes -- ./mia-data/notes
cargo run -p mia-browser
cargo run -p mia-calendar -- ./mia-data/calendar auth   # prima l'auth
cargo run -p mia-mail -- ./mia-data/mail
```

## Note

- Le scritture passano dal cancello di `mia-mcp`: un tool non in
  `read_only_tools` gira solo se `write_policy` lo permette (`deny` di
  default). `send-message` non parte mai senza conferma esplicita.
- I token OAuth stanno solo in `<data>/token.json` (volume nei
  container); le credenziali non entrano mai nel repo né nei log.
- `mia-browser` è un lettore (niente JavaScript): il rendering JS è un
  passo successivo, non un segnaposto.
- `mia-calendar` e `mia-mail` senza credenziali partono comunque: i
  tool rispondono con l'errore che spiega come autorizzare.
