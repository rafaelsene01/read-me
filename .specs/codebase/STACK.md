# Tech Stack

**Analyzed:** 2026-07-25 (após M3) · **revisado em 2026-07-28** contra o código de hoje.

Os números abaixo estavam defasados de vários milestones: "30 comandos", "150 testes + 9 ignorados", "4 stores", "CI: nenhum" e a ausência do `tauri-plugin-updater`. Cada valor novo traz a data em que foi medido e o comando que o mediu.

## Pré-requisitos de build (além do Rust e do Node)

- **protoc** (Protocol Buffers compiler) — exigido pelo `lance-encoding`, dependência do `lancedb`. Sem ele o `cargo build` falha com *"Could not find `protoc`"*. No Windows: `winget install Google.Protobuf` (verificado com 35.1). No Linux: `apt install protobuf-compiler`.
- **ONNX Runtime** — **não** é pré-requisito de build: o `fastembed` roda em modo `ort-load-dynamic`, porque a lib estática do ORT exige a STL do MSVC 2022 (VS 2019 Build Tools não serve).
  **Corrigido em 2026-07-28:** esta linha dizia que *"o app baixa o `onnxruntime.dll`/`.so` na primeira indexação"*. Isso deixou de ser verdade no M9 (SELF-12): a biblioteca viaja dentro do instalador e é resolvida por `runtime::bundled::onnxruntime_dylib` a partir do `resource_dir`; `rag::onnxruntime::ensure_dylib` só aponta o `ORT_DYLIB_PATH` para ela. O que é pré-requisito de *build* é rodar `npm run vendor` (o `beforeBuildCommand` do Tauri faz isso), que popula `src-tauri/resources/`.

## Core

- Framework: Tauri 2 (Rust backend + webview nativo do SO) — AD-001
- Language: Rust (edition 2021) no backend; TypeScript ~5.8 no frontend
- Runtime: webview do SO (WebView2 no Windows / WebKitGTK no Linux); binário Rust como processo host
- Package manager: npm (frontend) + cargo (backend)
- Crate name: `tauri-app` / lib `tauri_app_lib`; produto `ReadMe`, identifier `com.readme.app`

## Frontend

- UI Framework: React 19 (`react` ^19.1, `react-dom` ^19.1)
- Build: Vite 7 (`@vitejs/plugin-react`), dev server fixo em `:1420` (exigido por `tauri.conf.json` `devUrl`)
- Styling: Tailwind CSS v4 via `@tailwindcss/postcss` — **sem `tailwind.config.js`** (config CSS-first, AD-006). Temas por CSS variables em `src/styles/themes.css`
- State Management: Zustand ^5 — **6 stores** independentes, sem store raiz: `chatStore`, `configStore`, `uiStore`, `runtimeStore`, `documentsStore`, `updateStore` (contado em 2026-07-28 por `grep -rn "export const use[A-Za-z]*Store" src/store/`)
- i18n: i18next ^26 + react-i18next ^17 — EN default, PT disponível (AD-007)
- Ícones: lucide-react ^1.26
- Form Handling: nenhuma lib — `useState` + `onSubmit` manual
- Pontes com o backend: `@tauri-apps/api` ^2, `@tauri-apps/plugin-dialog` ^2.7, `@tauri-apps/plugin-opener` ^2

## Backend

- API Style: comandos Tauri (`#[tauri::command]` + `invoke_handler`), não HTTP. **39 comandos** registrados em `lib.rs` (contados em 2026-07-28 por `grep -rn "#\[tauri::command\]" src-tauri/src/ | wc -l`, que bate com o `generate_handler![]`)
- Database: SQLite via `rusqlite` 0.31 (feature `bundled` — compila o SQLite junto, sem dependência do SO). Sem ORM; SQL literal com `params![]`
- HTTP client: `reqwest` 0.12 (features `json`, `stream`) para falar com o sidecar `llama-server` em `127.0.0.1` e para baixar modelos GGUF
- Async: `tokio` 1 (features `sync`, `time`, `rt`, `macros`) + `tauri::async_runtime`; `futures-util` 0.3 para `bytes_stream()`. **`async-trait` saiu na AD-042** junto com o trait `ProviderClient` — confirmado ausente do `Cargo.toml` em 2026-07-28; não há mais despacho dinâmico
- Serialização: `serde` 1 (derive) + `serde_json` 1
- Geração de tipos: **`ts-rs` 12** (adicionado em 2026-07-28), dependência normal e não de dev, porque o `#[derive(TS)]` vive nas structs da própria lib. Existe para resolver o C-03 — gerar `src/types.ts` em vez de espelhá-lo à mão. A feature `serde-compat` (padrão) é o que faz ele ler `#[serde(flatten)]`/`rename_all` dos mesmos atributos do serde
- IDs: `uuid` 1 (v4); timestamps `chrono` 0.4 em RFC3339 (string)
- Sistema: `sysinfo` 0.39 (RAM total); `windows-sys` 0.61 (`Win32_System_JobObjects`) só no target Windows, para matar o sidecar por Job Object
- Empacotamento/update: `zip` 2 (bundle portátil) e `minisign-verify` 0.2 (assina/verifica o bundle portátil com a mesma chave do `tauri-plugin-updater`)

### Stack de RAG (M5)

- Embeddings: `fastembed` 5 com `default-features = false` e features `ort-load-dynamic` + `hf-hub-native-tls`
- Banco vetorial: `lancedb` 0.31 + `arrow-array` 58 + `arrow-schema` 58
- Parsing: `pdfium-render` 0.9.3 (PDF) e `docx-rs` 0.4 (DOCX); TXT/MD direto

## Plugins Tauri

- `tauri-plugin-opener` 2 (do template, pouco usado)
- `tauri-plugin-dialog` 2 (`pick_folder` no wizard/configurações)
- **`tauri-plugin-updater` 2.10** — atualização dos flavors instalados (`.msi` / NSIS / `.AppImage`). O flavor portátil não tem alvo nesse plugin, e é por isso que `src-tauri/src/update/portable.rs` existe. O piso `>= 2.10` é pelas chaves `{os}-{arch}-{installer}` do `latest.json`
- Capability única `default` (`src-tauri/capabilities/default.json`): `core:default`, `opener:default`, `dialog:default`, `updater:default`

## Testing

Números remedidos na **run 002 (2026-07-28)**, com a árvore parada. A medição anterior avisava que outro agente estava editando `src-tauri/src/**` na mesma janela — e estava certa em avisar: a contagem Rust subiu de 177/15 para 181/16 quando aquele trabalho terminou.

- Unit (Rust): `cargo test --lib` → **181 passando / 0 falhas / 16 ignorados**, todos em `#[cfg(test)] mod tests` co-locados
- Unit (frontend): **Vitest 4.1** + `@testing-library/react` 16 + `@testing-library/dom` 10 + `jsdom` 29, config em `vitest.config.ts`, setup em `src/test/setup.ts` e dublês em `src/test/doubles/`. `npx vitest run` → **63 testes em 8 arquivos, 0 falhas**. Isto derruba a linha "nenhum framework de teste instalado" que este documento carregava e é o C-04 do CONCERNS
- Unit (scripts Node): `npm run test:scripts` (`node --test scripts/*.test.mjs`) → **49 passando / 0 falhas**
- Integration: nenhum runner configurado (não há runner de integração Tauri)
- E2E: nenhum
- Detalhes e gates: `.specs/codebase/TESTING.md`

## External Services

**Nenhum.** Desde o M9 o app não fala com programa externo algum. O único runtime é o `llama-server` que viaja no instalador (`resources/llama/{vulkan,cpu}/`) e roda como processo filho em `127.0.0.1`.

O que sai da máquina, sempre por ação explícita do usuário: o download de um modelo GGUF (Hugging Face) e a verificação de atualização (GitHub Releases, com toggle de opt-out). Nenhum serviço de nuvem, telemetria ou auth externa.

## Development Tools

- Compilador Rust: rustc/cargo **1.97.1** (medido em 2026-07-28 por `rustc --version`), toolchain `stable-x86_64-pc-windows-msvc` (MSVC Build Tools necessários no Windows)
- Node: **>= 22** obrigatório (`engines` do `package.json`) — o `npm run test:scripts` depende de expansão de glob que versões anteriores não têm. Máquina de desenvolvimento medida em v24.12.0
- CLI: `@tauri-apps/cli` ^2 (`npm run tauri dev` / `build`)
- Type check: `tsc` 5.8.3 roda antes do Vite build (`npm run build` = `tsc && vite build`)
- Linter/formatter: **nenhum configurado** (sem ESLint, Prettier, rustfmt.toml, clippy.toml) — ver CONCERNS.md
- CI: **existe** desde o M8 — `.github/workflows/ci.yml` e `.github/workflows/release.yml`
  - `ci.yml`: roda em `push` para `master` e em todo `pull_request`; três jobs — `frontend` (`npm run build` + `npm run test:scripts`, ubuntu-latest, Node 24), `rust` (`cargo test`, ubuntu-22.04) e `commits` (valida Conventional Commits). **Só valida: nunca cria tag, release ou artefato**
    - **Buraco medido em 2026-07-28:** o job `frontend` tem exatamente dois passos de teste — `npm run build` e `npm run test:scripts`. **`npm test` (Vitest) não é chamado em lugar nenhum do `ci.yml`**, então os 63 testes de frontend não são gate de PR
  - `release.yml`: **`workflow_dispatch` manual**, de propósito — nenhum push publica nada. Jobs `prepare` → `build` → `finalize` → `cleanup`
- Empacotamento de release: `cliff.toml` (git-cliff, changelog) e `scripts/*.mjs` (`bump-version`, `make-portable`, `patch-latest-json`, `check-linux-bundle`, `vendor-runtime`)
