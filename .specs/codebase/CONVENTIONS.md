# Code Conventions

Observado em todo o `src-tauri/src/` e `src/`. Convenções **em uso**, não ideais — não há linter/formatter configurado, então elas são mantidas por consistência manual.

**Revisado em 2026-07-28.** Até esta data o arquivo ainda ensinava a copiar `ConnectionsPanel.tsx`, `connectionsApi.ts`, `useConnectionsStore` e `list_connections` — tudo removido no M9 (AD-039/AD-042). Todo exemplo abaixo foi re-conferido por `grep` contra o código de hoje.

## Naming Conventions

**Arquivos Rust:** `snake_case.rs`. Comandos Tauri sempre em arquivo com sufixo `_commands`.
Exemplos: `config_commands.rs`, `runtime_commands.rs`, `chat_commands.rs`, `document_commands.rs`, `update_commands.rs`, `system_info.rs`, `memory_estimate.rs`
**Exceção herdada:** `commands.rs` (o CRUD de chat do M1) não tem prefixo — é o único, e não é modelo a seguir.

**Arquivos React:** `PascalCase.tsx`, um componente exportado por arquivo, nome do arquivo = nome do componente.
Exemplos: `RuntimeCard.tsx`, `ModelDownloadCard.tsx`, `SettingsSection.tsx`, `ContextGauge.tsx`, `DocumentStatusBadge.tsx`

**Módulos de apoio TS:** `camelCase.ts`, com sufixo por papel — `*Api.ts` para wrappers `invoke`, `*Store.ts` para Zustand.
Exemplos: `runtimeApi.ts`, `chatApi.ts`, `documentsApi.ts`, `updateApi.ts`, `chatStore.ts`, `theme.ts`

**Funções Rust:** `snake_case`, verbo primeiro. Comandos Tauri usam o mesmo nome que o frontend invoca.
Exemplos: `list_installed_models`, `set_active_model`, `total_ram_gb`, `estimate_ram_gb`, `ensure_folder_structure`

**Funções/hooks TS:** `camelCase`; stores exportados como `useXStore`; handlers de evento como `handleX`.
Exemplos: `useRuntimeStore`, `useChatStore`, `loadDownloadableModels`, `handleChangeFolder`, `handleManualDownload`

**Constantes:** `SCREAMING_SNAKE_CASE` nos dois lados.
Exemplos: `MIGRATIONS`, `SUBDIRS`, `CURATED_MODELS`, `SUPPORTED_EXTENSIONS`, `GLOBAL_NAMESPACE` (Rust); `SUPPORTED_THEMES`, `SUPPORTED_LANGUAGES`, `DEFAULT_LANGUAGE`, `THEME_LABEL_KEYS` (TS)

**Campos que cruzam a fronteira:** `snake_case` (o serde não renomeia). Por isso `src/types.ts` tem `size_bytes`, `estimated_ram_gb`, `context_length`, `use_global_rag` — quebrando o camelCase idiomático do TS de propósito, pra bater com o Rust.
**Exceção:** parâmetros de `invoke()` são `camelCase` no TS e chegam `snake_case` no Rust — o Tauri faz essa conversão sozinho (`invoke("set_active_model", { modelName })` → `model_name: String`).

## Code Organization

**Marcador `SPEC:` no topo do arquivo**, antes dos imports, ligando o arquivo aos requisitos que ele implementa (regra em `.claude/rules/spec-driven-changes.md`). Presente em **50 arquivos** de `src/` e `src-tauri/src/` (remedido na run 002 por `grep -rl "^// SPEC:" src src-tauri/src`; eram 44 antes das features `generated-types` e `frontend-testing`).
```rust
// SPEC: self-contained-runtime (SELF-01, SELF-02, SELF-07, SELF-08)
```

**Ordem de imports (Rust):** `crate::` primeiro, depois crates externos em ordem alfabética.
```rust
use crate::config;
use crate::db::{require_conn, DbState};
use crate::providers::{PullProgress, PullStatus};
use crate::runtime::{bundled, detect, model, Backend, RuntimeError, TargetOs};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};
```

**Ordem de imports (TS):** React → libs externas → ícones → stores/lib internos → componentes → `type` imports por último.
```tsx
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Cpu, Download, Play, Square } from "lucide-react";
import { useRuntimeStore } from "../../store/runtimeStore";
```

**Estrutura de arquivo Rust:** imports → tipos/structs → `impl` → funções livres → `#[cfg(test)] mod tests` **no fim do mesmo arquivo** (nunca em `tests/` separado).

**Estrutura de componente React:** hooks primeiro (na ordem: `useTranslation`, stores, `useState`, `useEffect`), depois handlers, depois `return`. Early return antes do JSX — `if (!config) return null;` (`SettingsPanel.tsx:45`), `if (!status) return null;` (`RuntimeCard.tsx:17`).

## Type Safety / Documentation

**Rust:** tipos explícitos nas assinaturas públicas; `#[derive(Debug, Serialize, Clone)]` no que sai pro frontend, `Deserialize` no que entra. Structs de resposta HTTP são privadas ao módulo (`struct ModelsListResponse`, `ModelEntry`, `ModelMeta` em `llama_server.rs`) e nunca vazam pro frontend — sempre convertidas pro tipo comum (`InstalledModel`, `ModelLimits`).

**`ts-rs` (adicionado em 2026-07-28, C-03):** o que cruza a fronteira ganha `TS` no derive — `#[derive(Debug, Serialize, Clone, TS)]` — para gerar o lado TypeScript em vez de espelhá-lo à mão. Presente em **30 declarações**, em 15 arquivos Rust. **A migração terminou no mesmo dia** — esta frase dizia "em curso" porque foi escrita enquanto o outro agente ainda implementava: `src/types.ts` é hoje um **arquivo gerado**, com cabeçalho `GENERATED FILE — do not edit by hand`, produzido por `src-tauri/src/types_export.rs`. Regenere com `cd src-tauri && cargo test --lib types_export -- --ignored`. Editá-lo à mão é trabalho perdido: o gate `types_export::tests::types_ts_matches_rust_structs` compara bytes contra o que o Rust gera.

**TypeScript:** `strict` ligado (via `tsconfig.json`). Union types de string literal em vez de enum:
```ts
export type ActiveView = "chat" | "settings" | "runtime" | "documents";
export type DocumentStatus = "queued" | "parsing" | "chunking" | "embedding" | "ready" | "error";
export type RuntimeStage =
  | "unsupported" | "not_prepared" | "preparing" | "no_model" | "ready" | "running";
```
Isso força o `Record<Theme, string>` a mapear todas as chaves — TypeScript quebra o build se alguém adiciona um tema e esquece o label (comportamento aproveitado de propósito, AD-013).

## Error Handling

**Rust:** `Result<T, String>` em **toda** fronteira de comando — nunca `anyhow`, `thiserror` ou tipo de erro customizado exposto. Conversão via `.map_err(|e| e.to_string())`.
```rust
conn.execute(...).map_err(|e| e.to_string())?;
```
**Exceção deliberada:** `providers/` tem um enum real (`ProviderError { Unavailable, RequestFailed, ParseError }`) porque precisa **distinguir** "servidor offline" de "resposta malformada" — mas ele é achatado pra `String` na borda do comando.

**Mensagens de erro de usuário:** em **português**, mesmo com o código em inglês.
```rust
.ok_or_else(|| "Nenhuma pasta de armazenamento configurada ainda".to_string())
```

**TypeScript:** stores capturam e guardam como string, nunca propagam exception pra UI:
```ts
try { const chats = await chatApi.listChats(); set({ chats, isLoading: false }); }
catch (err) { set({ error: String(err), isLoading: false }); }
```
**Exceção:** `runtimeStore.configureModel` (`src/store/runtimeStore.ts:143`) **não** captura — a exception sobe para quem chamou. Quem trata é o `ModelConfigForm.handleSave`, que precisa distinguir "salvou" de "falhou" no próprio formulário (`setSaved` / `setError`), coisa que um `error` global do store não expressa.

## Comments / Documentation

**Estilo:** comentários explicam **por quê**, nunca o quê. Densidade baixa — a maioria das funções não tem nenhum. Usados quase só para:

1. **Decisões não-óbvias**, geralmente citando o motivo real:
```rust
/// Small bootstrap pointer file. It only stores *where* the user chose to put
/// […] This indirection is what lets the storage folder be reconfigurable
/// without knowing it in advance.
```

2. **Rastreabilidade para a spec**, com o marcador `// SPEC: <feature> (<IDs>)` no topo do arquivo (item 3 de `.claude/rules/spec-driven-changes.md`). Um arquivo que implementa requisito de mais de uma feature lista as duas:
```rust
// SPEC: app-shell (SHELL-08), chat-messaging (CHAT-11), documents-rag (DOC-02),
//       self-contained-runtime (SELF-06), conversation-memory (MEM-15, MEM-16)
```
> **Removido em 2026-07-28.** Este item descrevia um segundo marcador, `// SPEC_DEVIATION:`, para divergências entre spec e código. Ele tem **zero ocorrências** em `src/` e `src-tauri/src/` hoje (`grep -rn "SPEC_DEVIATION"`), então deixou de ser convenção observada — não foi substituído por outro marcador de divergência.

3. **Fatos verificados** que ficariam invisíveis no código:
```rust
/// `System::total_memory()` returns bytes (since sysinfo 0.26.0).
```

**Idioma:** comentários e commits em **inglês**; strings de UI e mensagens de erro pro usuário em **português** (ou chave i18n). Docs em `.specs/` em português.

## i18n

Nenhuma string literal visível ao usuário no JSX — sempre `t("chave.aninhada")`, com a chave adicionada nos **dois** arquivos (`en.json` e `pt.json`) no mesmo commit. Interpolação no padrão i18next: `t("chatPanel.contextUsage", { used: format(tokens), max: format(max) })` ↔ `"Context: {{used}} of {{max}} tokens"`.

Paridade medida em 2026-07-28: **148 chaves em cada arquivo, zero divergência**.

## Estilo visual

Zero cor hardcoded — sempre CSS variable via Tailwind arbitrary value:
```tsx
className="bg-[var(--bg-elevated)] text-[var(--text-secondary)] border-[var(--border-color)]"
```
As variáveis vivem em `src/styles/themes.css`, um bloco por `[data-theme=…]`.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/), escopo = nome da feature ou da área, corpo explicando o **porquê**. Um commit por task. O job `commits` do `.github/workflows/ci.yml` valida isso em PRs.
```
feat(runtime): implement self-contained runtime and sidecar lifecycle management
fix(chat): ensure chat deletion cancels ongoing generations
```
