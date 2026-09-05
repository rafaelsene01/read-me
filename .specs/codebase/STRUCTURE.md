# Project Structure

**Root:** `D:\chat-ia-local`

**Revisado em 2026-07-28** (`ls -R src/components`, `ls src-tauri/src`, `ls -a .`). A árvore anterior listava `src/components/Connections/` — diretório que não existe desde o M9 — e omitia `Documents/`, `Runtime/`, `Update/`, `src/test/`, `scripts/`, `.github/` e `docs/`.

## Directory Tree

```
chat-ia-local/
├── .github/workflows/          # ci.yml (validação) · release.yml (workflow_dispatch)
├── .specs/                     # Spec-driven docs (este sistema)
│   ├── project/                # PROJECT.md · ROADMAP.md · STATE.md
│   ├── codebase/               # Brownfield mapping (estes arquivos)
│   ├── features/               # spec.md/design.md/tasks.md por feature
│   ├── quick/ · runs/          # quick tasks e journal da skill spec-loop
├── .claude/                    # rules/ e skills/ locais do repositório
├── docs/RELEASING.md           # procedimento de release
├── scripts/                    # Node puro: bump-version · make-portable
│                               #   · patch-latest-json · check-linux-bundle
│                               #   · vendor-runtime (+ *.test.mjs) · vendor.json
├── src/                        # Frontend React + TS
│   ├── components/
│   │   ├── Chat/               # ChatPanel · ContextGauge · MessageInput
│   │   ├── Documents/          # DocumentsPanel · DocumentRow · DocumentStatusBadge
│   │   ├── Onboarding/         # Wizard.tsx
│   │   ├── Runtime/            # RuntimePanel · RuntimeCard · ModelsList
│   │   │                       #   · ModelDownloadCard · ModelConfigForm
│   │   ├── Settings/           # SettingsPanel.tsx
│   │   ├── Sidebar/            # Sidebar · ChatList · DocumentsSection
│   │   │                       #   · RuntimeSection · SettingsSection
│   │   └── Update/             # UpdateBanner.tsx
│   ├── i18n/                   # index.ts + locales/{en,pt}.json
│   ├── lib/                    # chatApi · configApi · documentsApi · runtimeApi · updateApi · theme
│   ├── store/                  # chatStore · configStore · uiStore · runtimeStore · documentsStore · updateStore
│   ├── styles/themes.css       # CSS variables por tema
│   ├── test/                   # setup.ts + doubles/ (Vitest)
│   ├── assets/ · App.css
│   ├── App.tsx · main.tsx · types.ts · index.css · vite-env.d.ts
├── src-tauri/                  # Backend Rust
│   ├── src/
│   │   ├── models/             # mod.rs (Chat/Message) · catalog.rs · memory_estimate.rs
│   │   ├── providers/          # mod.rs · llama_server.rs · openai_stream.rs
│   │   ├── runtime/            # bundled · detect · download · job · log
│   │   │                       #   · model · process · store
│   │   ├── rag/                # chunking · embedding · onnxruntime · parsing
│   │   │                       #   · pdfium · pipeline · store
│   │   ├── chat/               # attachments · cancellation · context_assembler · memory
│   │   ├── update/             # manifest · portable · signature
│   │   ├── lib.rs · main.rs
│   │   ├── commands.rs · chat_commands.rs · config_commands.rs
│   │   │   · document_commands.rs · runtime_commands.rs · update_commands.rs
│   │   └── config.rs · db.rs · system_info.rs
│   ├── resources/              # componentes vendorizados; só o `.gitkeep` é versionado
│   ├── capabilities/default.json
│   ├── icons/ · Cargo.toml · tauri.conf.json · build.rs
├── dist/                       # Build do Vite (gerado, gitignored)
├── public/ · index.html
├── CHANGELOG.md · cliff.toml · AGENTS.md · CLAUDE.md · README.md
└── package.json · tsconfig.json · tsconfig.node.json · vite.config.ts
    · vitest.config.ts · postcss.config.js
```

> **`src-tauri/resources/` não é inteiramente gitignored.** O `.gitignore` ignora o **conteúdo** (`src-tauri/resources/*`) e versiona `!src-tauri/resources/.gitkeep`, porque `bundle.resources` nomeia a pasta e o `tauri-build` quebra o build inteiro quando ela não existe — foi o defeito do CI da AD-049.

## Module Organization

### Comandos Tauri (fronteira frontend↔backend)

**Purpose:** Única porta de entrada do frontend pro backend. Todo `#[tauri::command]` vive num arquivo `*_commands.rs`, nunca misturado com lógica de domínio.
**Location:** `src-tauri/src/{commands,chat_commands,config_commands,document_commands,runtime_commands,update_commands}.rs`
**Key files:** `lib.rs` registra todos no `invoke_handler![]` — se não está lá, o frontend não enxerga.

### Domínio / lógica

**Purpose:** Lógica pura e orquestração, sem anotação Tauri — testável isoladamente.
**Location:** `src-tauri/src/{config,db,system_info}.rs`, `models/`, `providers/`, `runtime/`, `rag/`, `chat/`, `update/`
**Key files:** `providers/llama_server.rs` (o cliente HTTP do sidecar), `runtime/store.rs` (a linha singleton que responde "qual modelo, com qual contexto, com qual GPU"), `runtime/bundled.rs` (onde cada componente do instalador é encontrado)

### Camada de dados do frontend

**Purpose:** Espelhar cada comando Tauri num wrapper tipado e expor estado via Zustand.
**Location:** `src/lib/*Api.ts` (wrappers `invoke`) + `src/store/*Store.ts` (estado)
**Key files:** `src/types.ts` — todas as interfaces que cruzam a fronteira Rust↔TS moram aqui, num arquivo só.

### UI

**Purpose:** Componentes React, um diretório por área funcional.
**Location:** `src/components/<Área>/`
**Key files:** `App.tsx` faz o roteamento (não há react-router — é um switch em `uiStore.activeView`).

## Where Things Live

**Chats (M1):**
- UI: `src/components/Sidebar/ChatList.tsx`, `src/components/Chat/ChatPanel.tsx`
- Estado: `src/store/chatStore.ts` · API: `src/lib/chatApi.ts`
- Backend: `src-tauri/src/commands.rs` · Modelos: `src-tauri/src/models/mod.rs`

**Config/Storage/i18n (M2):**
- UI: `src/components/Onboarding/Wizard.tsx`, `src/components/Settings/SettingsPanel.tsx`
- Estado: `src/store/configStore.ts` · API: `src/lib/configApi.ts`
- Backend: `src-tauri/src/config.rs` + `config_commands.rs`
- Bootstrap: `config.json` no `app_config_dir` do SO (**fora** da pasta-base — AD-012)

> **Seção removida em 2026-07-28: "Conexões & Modelos (M3)".** Ela apontava para `src/components/Connections/*`, `Sidebar/ConnectionsSection.tsx`, `src/store/connectionsStore.ts`, `src/lib/connectionsApi.ts`, `connections.rs`, `connection_commands.rs` e `model_commands.rs` — **nenhum desses arquivos existe**. A feature saiu inteira no M9 (AD-039 planejou, AD-042 executou); o que ocupou o lugar dela é o Runtime embutido, abaixo. As tabelas `connections`/`model_configs` também caíram, pela `MIGRATION_7_SINGLE_RUNTIME`.

**Runtime embutido & Modelos (M9):**
- UI: `src/components/Runtime/*` + `Sidebar/RuntimeSection.tsx`
- Estado: `src/store/runtimeStore.ts` · API: `src/lib/runtimeApi.ts`
- Backend: `runtime_commands.rs`, `runtime/` (bundled · detect · download · job · log · model · process · store), `providers/`, `models/catalog.rs`, `system_info.rs`

**Documentos & RAG (M5):**
- UI: `src/components/Documents/*` + `Sidebar/DocumentsSection.tsx`
- Estado: `src/store/documentsStore.ts` · API: `src/lib/documentsApi.ts`
- Backend: `document_commands.rs`, `rag/` (chunking · embedding · onnxruntime · parsing · pdfium · pipeline · store)

**Chat, anexos e memória (M4/M6):**
- UI: `src/components/Chat/*`
- Estado: `src/store/chatStore.ts` · API: `src/lib/chatApi.ts`
- Backend: `chat_commands.rs`, `chat/` (attachments · cancellation · context_assembler · memory)

**Atualização (M8):**
- UI: `src/components/Update/UpdateBanner.tsx`
- Estado: `src/store/updateStore.ts` · API: `src/lib/updateApi.ts`
- Backend: `update_commands.rs`, `update/` (manifest · portable · signature) + `tauri-plugin-updater`

## Special Directories

**Pasta-base do usuário** (escolhida no wizard, fora do repo — AD-008):
**Purpose:** Todos os dados reais do usuário.
**Conteúdo:** `readme.db` (SQLite) mais as **5** subpastas de `config::SUBDIRS` — `models/`, `documents/`, `vectors/`, `chats/`, `runtime/` — criadas por `config::ensure_folder_structure` (conferido em `src-tauri/src/config.rs:126`).

**`src-tauri/target/`** e **`dist/`**: build artifacts, ambos gitignored.

**`.specs/`**: documentação do processo, não código — mas é a fonte da verdade sobre decisões (STATE.md).
