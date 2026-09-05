# Tipos gerados na fronteira Rust↔TS — Tasks

**Spec:** `.specs/features/generated-types/spec.md`
**Design:** `.specs/features/generated-types/design.md`
**Status:** executado em 2026-07-28 — o log com os números medidos fica no fim
do arquivo, junto com a lista do que **não** foi verificado

---

## Tasks

### T1 — Adicionar `ts-rs` e provar o `flatten` antes de qualquer outra coisa

**O quê:** `ts-rs = "12"` no `Cargo.toml`; `#[derive(TS)]` em
`CuratedModelInfo` e `DownloadableModel`; imprimir a declaração gerada.
**Onde:** `src-tauri/Cargo.toml`, `src-tauri/src/models/catalog.rs`,
`src-tauri/src/runtime_commands.rs`
**Done when:** a saída do `flatten` está medida e colada no `design.md`.
**Gate:** `cargo test --lib` compila.
**Por que primeiro:** se o `flatten` não funcionasse, a escolha de ferramenta
cairia e todo o resto seria retrabalho.

### T2 — Derivar `TS` em todo tipo que atravessa a fronteira

**O quê:** `#[derive(TS)]` nas 25 declarações; `#[ts(rename = …)]` nos dois
tipos cujo nome TS difere do Rust (`RuntimeProgress` → `RuntimeProgressEvent`,
`ModelDownloadProgress` → `ModelDownloadProgressEvent`, `DownloadProgress` →
`UpdateProgress`); `#[ts(type = "number")]` nos campos de 64 bits (TYPE-08).
**Onde:** 11 arquivos de `src-tauri/src/`
**Depends on:** T1
**Done when:** `cargo check --lib` limpo, sem warnings novos.

### T3 — Estreitamentos declarados no Rust (TYPE-07)

**O quê:** `#[ts(type = …)]` em `DocumentRecord.status`, `ChatAttachment.status`,
`RuntimeStatus.backend` e `Message.role`, cada um com comentário nomeando o
escritor; `#[derive(TS)]` em `runtime::Backend`.
**Onde:** `document_commands.rs`, `chat_commands.rs`, `runtime_commands.rs`,
`models/mod.rs`, `runtime/mod.rs`
**Depends on:** T2

### T4 — O módulo gerador e o gate

**O quê:** `src-tauri/src/types_export.rs` com `generate_types_ts()` em ordem
fixa, o teste de comparação (TYPE-03/04/09) e o teste `#[ignore]` que escreve.
**Onde:** `src-tauri/src/types_export.rs`, `src-tauri/src/lib.rs`
**Depends on:** T2, T3
**Tests:** unit — `types_ts_matches_rust_structs` (no gate padrão),
`regenerate_types_ts` (`#[ignore]`)

### T5 — Regenerar `src/types.ts` e fechar o frontend

**O quê:** rodar a regeneração, commitar o arquivo gerado.
**Onde:** `src/types.ts`
**Depends on:** T4
**Gate:** `npm run build` limpo, `cargo test --lib` no baseline + 2

### T6 — A prova (TYPE-06)

**O quê:** renomear um campo no Rust de propósito e medir o que falha.
**Depends on:** T5
**Done when:** existe a saída de comando mostrando o gate vermelho, e o
`revert` mostrando o verde de volta.

---

## Execution Log

Todas as linhas abaixo foram escritas **depois** de o artefato estar no disco e
o comando ter rodado. Onde não há número medido, está escrito que não há.

### T1 — `ts-rs` e a prova do `flatten` ✅

Feita numa sessão anterior, verificada nesta: `ts-rs = "12"` no `Cargo.toml`
(resolvido para 12.0.1 no `Cargo.lock`), `#[derive(TS)]` em `CuratedModelInfo`,
`Chat`, `Message` e `DownloadableModel`.

A saída do `flatten` colada no `design.md` **estava errada** — afirmava uma
interseção `{ fits_ram: boolean } & CuratedModelInfo`, que não é o que o `ts-rs`
12.0.1 produz. Corrigida nesta sessão com a saída real, que é o objeto inlinado.
O requisito TYPE-05 continua satisfeito por outro caminho; ver `design.md`.

### T2 — `TS` em todo tipo que atravessa a fronteira ✅

**Auditoria.** `grep -rn "derive(.*Serialize" src-tauri/src/` devolve **30**
declarações. Critério aplicado: entra quem é retorno de `#[tauri::command]` ou
payload de `emit`; um `Serialize` que só serve a outro destino não entra.

- **29 entraram.**
- **1 descartada:** `providers::ChatMessage`. É o corpo HTTP do
  `/v1/chat/completions` que o app manda para o `llama-server`
  (`providers::openai_stream`, `providers::llama_server`); nenhum comando a
  retorna e nenhum `emit` a carrega. Confirmado por `grep -rn "ChatMessage"`:
  todos os usos estão em `chat/context_assembler.rs` e `providers/`.

Três não-derives ficaram de fora pelo mesmo critério: `providers::GpuOffload`
tem `impl Serialize` à mão e vira string numa coluna do SQLite; as structs de
`chat/context_assembler.rs` não serializam.

**Renames aplicados** (`#[ts(rename = …)]`), os três que o plano previa:
`RuntimeProgress` → `RuntimeProgressEvent`, `ModelDownloadProgress` →
`ModelDownloadProgressEvent`, `portable::DownloadProgress` → `UpdateProgress`.
Os dois primeiros eram `struct` privadas e passaram a `pub(crate)` para o
gerador alcançá-las.

**TYPE-08:** nove campos de 64 bits anotados com `#[ts(type = "number")]` /
`"number | null"`, listados no `design.md`.

**SPEC_DEVIATION — contagem.** O `tasks.md` dizia "as 25 declarações" e o
`spec.md` dizia 24. O conjunto real gerado é **30**: as 29 acima mais
`runtime::Backend`, que não tem `Serialize` nenhum e entra só como alvo do
estreitamento do TYPE-07. O `src/types.ts` escrito à mão tinha 29 declarações;
sai `ChatAttachmentStatus` (o alias virou um `#[ts(type = …)]` inline) e entram
`CuratedModelInfo` e `Backend`.

Gate: `cargo check --lib` → `Finished dev profile in 33.53s`, zero warnings.

### T3 — Estreitamentos declarados (TYPE-07) ✅

Cada `#[ts(type = …)]` tem, ao lado, um comentário nomeando o escritor — e cada
escritor foi achado por `grep`, não presumido:

| Campo | Escritores achados | Tipo gerado |
| --- | --- | --- |
| `DocumentRecord.status` | `rag::pipeline::set_status` (`DocumentStatus::as_str()`) + os dois `INSERT` que semeiam a linha: `document_commands::import_documents` e `chat::attachments::index_large_attachment`, ambos com `queued` | `DocumentStatus` |
| `ChatAttachment.status` | três: `chat::attachments::record` (`"error"`, `"injected_whole"`, `"queued"`), `chat::attachments::finish_attachment` (**copia** de `documents.status`) e `document_commands::FAIL_INTERRUPTED_ATTACHMENTS` (`'error'`) | `DocumentStatus \| "injected_whole"` |
| `RuntimeStatus.backend` | `runtime_commands::prepare_runtime`, que grava `Backend::as_str()` em `embedded_runtime.backend`; `status_from` só repassa a linha | `Backend \| null` |
| `Message.role` | `chat_commands::insert_message` | `"user" \| "assistant" \| "system"` |

`runtime::Backend` ganhou `#[derive(ts_rs::TS)]` + `#[ts(rename_all = "lowercase")]`.
Esse `rename_all` **repete** o mapeamento que `as_str()` faz, e nada no
compilador liga os dois — então liga um teste:
`types_export::tests::backend_as_str_matches_generated_union`.

### T4 — O módulo gerador e o gate ✅

`src-tauri/src/types_export.rs` criado, com `#[cfg(test)] mod tests` no fim do
próprio arquivo. Declarado em `lib.rs` como `#[cfg(test)] mod types_export;`.

Quatro testes no gate padrão + um `#[ignore]`:

```
running 5 tests
test types_export::tests::regenerate_types_ts ... ignored, writes src/types.ts; run explicitly after changing a struct
test types_export::tests::backend_as_str_matches_generated_union ... ok
test types_export::tests::types_ts_matches_rust_structs ... ok
test types_export::tests::no_bigint_reaches_the_frontend ... ok
test types_export::tests::generation_is_deterministic ... ok

test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 192 filtered out
```

**SPEC_DEVIATION — número de testes.** O plano previa 2 (o comparador e o
`#[ignore]`), o que daria baseline + 2 = 179. Saíram **4 + 1**, e o resultado é
181. Os dois a mais existem porque cada um fecha um buraco que o comparador
sozinho não fecha:

- `backend_as_str_matches_generated_union` — sem ele, mudar `Backend::as_str()`
  deixaria `RuntimeStatus.backend` estreitado para strings que o backend não
  manda mais, **e o comparador passaria**, porque o arquivo gerado continuaria
  igual ao commitado;
- `no_bigint_reaches_the_frontend` — o `#[ts(type = "number")]` por campo depende
  de quem adicionar o *próximo* `u64` saber da regra; este teste não depende.

### T5 — Regenerar `src/types.ts` ✅

```
$ cargo test --lib types_export -- --ignored
running 1 test
test types_export::tests::regenerate_types_ts ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 196 filtered out
```

Gates, todos executados nesta sessão:

| Gate | Resultado medido |
| --- | --- |
| `cd src-tauri && cargo test --lib` | **181 passed; 0 failed; 16 ignored** (baseline 177/15) |
| `npm run build` | `✓ 1859 modules transformed` / `✓ built in 5.45s`, tsc sem erro |
| `npm test` | `Test Files 8 passed (8)` / `Tests 63 passed (63)` |

**Nenhum arquivo do frontend precisou ser tocado.** O `.tsx` e os stores
compilaram contra o arquivo gerado sem alteração — inclusive
`ModelDownloadCard.tsx` e `ModelsList.tsx`, que consomem o `DownloadableModel`
inlinado.

### T6 — A prova (TYPE-06) ✅

Rename deliberado: `CuratedModelInfo.estimated_ram_gb` → `estimated_ram_gigabytes`
em `models/catalog.rs`, propagado para o único outro uso
(`runtime_commands.rs:484`) para que o Rust continuasse compilando — que é o
ponto do cenário.

**Os dois compiladores ficam calados.** É exatamente o modo de falha do C-03:

```
$ cargo check --lib
    Checking tauri-app v0.2.0 (D:\chat-ia-local\src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.40s

$ npm run build
✓ 1859 modules transformed.
dist/assets/index-CnT4VmT_.js   316.07 kB │ gzip: 96.23 kB
✓ built in 5.23s
```

**O gate, vermelho:**

```
$ cargo test --lib types_export
running 5 tests
test types_export::tests::regenerate_types_ts ... ignored, writes src/types.ts; run explicitly after changing a struct
test types_export::tests::backend_as_str_matches_generated_union ... ok
test types_export::tests::no_bigint_reaches_the_frontend ... ok
test types_export::tests::generation_is_deterministic ... ok
test types_export::tests::types_ts_matches_rust_structs ... FAILED

failures:

---- types_export::tests::types_ts_matches_rust_structs stdout ----

thread 'types_export::tests::types_ts_matches_rust_structs' (1892) panicked at src\types_export.rs:147:13:
src/types.ts is out of sync with the Rust structs.
first differing line (67):
  committed:  pull_identifier: string, params_billions: number, default_quant: string, estimated_ram_gb: number,
  generated:  pull_identifier: string, params_billions: number, default_quant: string, estimated_ram_gigabytes: number,

Regenerate it with:
    cd src-tauri && cargo test --lib types_export -- --ignored
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    types_export::tests::types_ts_matches_rust_structs

test result: FAILED. 3 passed; 1 failed; 1 ignored; 0 measured; 192 filtered out; finished in 0.00s

error: test failed, to rerun pass `--lib`
```

Repare na linha 67: a divergência foi apontada **dentro do `CuratedModelInfo`**,
que mora em outro módulo e em outro arquivo. É o AC2 da segunda user story.

**Revertido, e verde de volta:**

```
$ cargo test --lib types_export
running 5 tests
test types_export::tests::regenerate_types_ts ... ignored, writes src/types.ts; run explicitly after changing a struct
test types_export::tests::backend_as_str_matches_generated_union ... ok
test types_export::tests::types_ts_matches_rust_structs ... ok
test types_export::tests::no_bigint_reaches_the_frontend ... ok
test types_export::tests::generation_is_deterministic ... ok

test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 192 filtered out; finished in 0.00s
```

---

## O que NÃO foi verificado

- **O app nunca foi executado.** Nada aqui prova que a tela de modelos, a lista
  de documentos ou a barra de download continuam desenhando certo em runtime.
  O que está provado é que o `tsc` aceita o arquivo gerado e que os 63 testes de
  frontend passam contra ele — nenhum dos dois é a tela.
- **`ChatAttachment.status` ficou mais largo do que era**
  (`DocumentStatus | "injected_whole"` no lugar de quatro literais). Os dois
  únicos usos no frontend são `a.status === "error"` e `!== "error"`, que
  compilam contra qualquer união — ou seja, o `npm run build` limpo **não é
  prova** de que a UI trata os estados novos (`parsing`, `chunking`,
  `embedding`). Ela provavelmente não trata; corrigir isso é a outra feature que
  o `spec.md` já deixou fora de escopo.
- **O custo do `ts-rs` no binário de release não foi medido.** O `design.md`
  aceita o trade-off e diz "custo medido: ver tasks.md" — não há esse número.
  `cargo build --release` não foi rodado nesta sessão. O `types_export.rs` em si
  é `#[cfg(test)]` e não entra; os `impl TS` dos derives entram.
- **Nada foi commitado.** Tudo está no working tree.
