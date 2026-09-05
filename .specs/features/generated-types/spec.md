# Tipos gerados na fronteira Rust↔TS — Especificação

**Design:** `.specs/features/generated-types/design.md`
**Tasks:** `.specs/features/generated-types/tasks.md`
**Status:** Implementada (ver rastreabilidade no fim)
**Origem:** C-03 do `.specs/codebase/CONCERNS.md`
**Prefixo de ID:** `TYPE`

---

## Problem Statement

`src/types.ts` espelha à mão as structs Rust que atravessam a fronteira do IPC.
São 29 declarações escritas por uma pessoa a partir de outra pessoa ter olhado
as structs, e a correspondência é mantida só por disciplina — o `AGENTS.md`
chega a dizer, em letra de forma, *"mudou uma, mude a outra — não há geração"*.

O modo de falha é preciso e silencioso: renomear um campo no Rust **compila**,
o `tsc` **também compila** (ele não sabe da existência da struct), e a quebra
só aparece quando o app roda, como `undefined` na tela. Não existe teste que
pegue isso — o `TESTING.md` põe comandos Tauri e componentes React na coluna
"nenhum teste".

O caso mais frágil é o `DownloadableModel`. No Rust ele é

```rust
pub struct DownloadableModel {
    #[serde(flatten)]
    pub info: crate::models::catalog::CuratedModelInfo,
    pub fits_ram: bool,
}
```

e no TS é uma **interface plana** com os sete campos do `CuratedModelInfo` mais
`fits_ram`. Não há nada no código que ligue as duas formas: a correspondência
existe porque alguém sabia o que `#[serde(flatten)]` faz e digitou o resultado.
Renomear `estimated_ram_gb` no `CuratedModelInfo` — uma struct em outro arquivo,
em outro módulo — quebra a tela de modelos sem que nada acuse.

Esta feature troca a disciplina por um gate.

## Goals

- [x] `src/types.ts` deixa de ser escrito à mão e passa a ser **gerado** das structs Rust
- [x] Divergência entre o arquivo commitado e o que as structs produzem **falha o gate padrão** (`cargo test --lib`)
- [x] O caso `#[serde(flatten)]` é representado fielmente, sem alguém precisar saber o que `flatten` faz
- [x] O ganho é **demonstrado**, não afirmado: um rename no Rust que antes passava nos dois lados agora derruba um teste

## Out of Scope

| Item | Motivo |
| --- | --- |
| Gerar os **wrappers de `invoke`** (`*Api.ts`) a partir dos `#[tauri::command]` | É o que `tauri-specta` faz e é outro problema — o do nome do comando como string (registrado no C-03 vizinho e no comentário de `runtimeApi.ts`). Fica para uma spec própria; misturar as duas dobraria o raio de mudança e obrigaria a mexer em `src/store/**` e `src/components/**` |
| Gerar tipos dos payloads de **evento** por um canal tipado | O nome do evento continua string dos dois lados; gerar o *shape* do payload (que é o que esta spec faz) já é o grosso do risco |
| Tipar `Result<T, String>` como união discriminada no TS | Convenção estabelecida em `CONVENTIONS.md`: erro atravessa como `String` e o store captura. Mudar isso é decisão de arquitetura, não de codegen |
| Corrigir o modelo de dados de `chat_attachments.status` | Esta spec **expõe** que a união escrita à mão estava incompleta (ver TYPE-07), mas corrigir o schema é trabalho de outra feature |

---

## User Stories

### P1: Um rename no Rust não chega em produção calado ⭐ MVP

**User Story:** Como mantenedor, quero que renomear um campo de uma struct que
atravessa o IPC quebre um teste, para não descobrir isso como `undefined` na
tela depois de publicar.

**Why P1:** É o risco literal do C-03 e a única razão de a feature existir.

**Acceptance Criteria:**

1. WHEN uma struct exportada muda de campo (rename, remoção, adição ou troca de tipo) e `src/types.ts` não é regenerado THEN o sistema SHALL falhar em `cargo test --lib`, apontando o tipo divergente
2. WHEN o arquivo commitado corresponde às structs THEN o sistema SHALL passar sem escrever nada em disco
3. WHEN o teste falha THEN a mensagem SHALL conter o comando exato de regeneração

**Independent Test:** renomear um campo numa struct, rodar `cargo test --lib`,
observar a falha; desfazer, rodar de novo, observar o verde.

---

### P1: `flatten` deixa de ser conhecimento tácito ⭐ MVP

**User Story:** Como mantenedor, quero que a forma achatada do
`DownloadableModel` venha do próprio `#[serde(flatten)]`, para não depender de
alguém lembrar o que aquele atributo faz com o JSON.

**Why P1:** É o caso que o C-03 nomeia como o mais frágil, e o que decide a
escolha de ferramenta.

**Acceptance Criteria:**

1. WHEN `DownloadableModel` é gerado THEN o tipo TS resultante SHALL expor os campos de `CuratedModelInfo` no mesmo nível de `fits_ram`, sem ninguém ter transcrito a lista
2. WHEN um campo de `CuratedModelInfo` é renomeado THEN o gate SHALL falhar, mesmo o `CuratedModelInfo` morando em outro módulo
3. WHEN o código é compilado THEN o `npm run build` SHALL continuar limpo com o tipo gerado (o consumo em `ModelDownloadCard.tsx` e `ModelsList.tsx` não muda)

**Independent Test:** renomear `estimated_ram_gb` em `models/catalog.rs` e ver o
gate falhar; conferir que o `.ts` gerado tem `fits_ram` e os campos do
`CuratedModelInfo` acessíveis pelo mesmo `m.estimated_ram_gb` de hoje.

---

### P2: O arquivo gerado não mente para o TypeScript

**User Story:** Como mantenedor, quero que o `types.ts` gerado prometa só o que
o Rust entrega, para não trocar um erro silencioso por outro.

**Why P2:** O arquivo escrito à mão **estreita** vários campos além do que o
Rust garante (`status: String` virando união de literais). Parte disso é
legítima — existe um enum Rust que é o único escritor do valor — e parte é
chute. Gerar sem olhar isso propagaria o chute com cara de automático.

**Acceptance Criteria:**

1. WHEN um campo é `String` no Rust e o TS o estreita para uma união THEN o estreitamento SHALL estar declarado no Rust, por atributo, com um comentário nomeando o código que garante o conjunto de valores
2. WHEN não existe escritor único identificável THEN o tipo gerado SHALL ser o tipo honesto (`string`), e a divergência com o que estava escrito à mão SHALL ser registrada aqui
3. WHEN um inteiro de 64 bits atravessa THEN o tipo gerado SHALL ser `number`, porque o IPC do Tauri serializa em JSON

---

## Requirements

| ID | Requisito |
| --- | --- |
| TYPE-01 | `src/types.ts` é **gerado** a partir das structs/enums Rust, e se anuncia como gerado no topo do arquivo |
| TYPE-02 | Todo tipo que atravessa a fronteira (retorno de `#[tauri::command]` ou payload de `emit`) está no conjunto gerado |
| TYPE-03 | Um teste do gate padrão (`cargo test --lib`) compara o arquivo commitado com o que as structs produzem e **falha** na divergência |
| TYPE-04 | Regenerar o arquivo é um comando único, documentado na mensagem de falha do próprio teste |
| TYPE-05 | `#[serde(flatten)]` é honrado pela geração — `DownloadableModel` sai com os campos de `CuratedModelInfo` no mesmo nível de `fits_ram` |
| TYPE-06 | O teste de comparação é **provado** contra um rename real: uma mudança que antes compilava dos dois lados agora derruba o gate |
| TYPE-07 | Nenhum estreitamento de tipo sem escritor Rust nomeado; os estreitamentos que sobrevivem estão declarados por atributo, ao lado do campo |
| TYPE-08 | `u64`/`i64`/`usize` chegam ao TS como `number`, não `bigint` — o IPC do Tauri é JSON |
| TYPE-09 | A ordem do arquivo gerado é determinística: rodar a geração duas vezes produz bytes idênticos |

---

## Rastreabilidade

Medida em 2026-07-28, com o working tree no estado descrito no `tasks.md`.

| ID | Onde | Verificação |
| --- | --- | --- |
| TYPE-01 | `src-tauri/src/types_export.rs`, `src/types.ts` | ✅ Arquivo regenerado (não commitado — o repositório deixa isso para o mantenedor); cabeçalho `// GENERATED FILE — do not edit by hand.` na linha 1 |
| TYPE-02 | `src-tauri/src/types_export.rs` (`generate_types_ts`) | ✅ **30** declarações emitidas, não 25: 29 dos 30 `#[derive(Serialize)]` do backend mais `runtime::Backend`. A única descartada é `providers::ChatMessage` (corpo HTTP do llama-server, não IPC) — critério e conferência no `tasks.md` T2 |
| TYPE-03 | `types_export::tests::types_ts_matches_rust_structs` | ✅ Executado: verde no estado normal, vermelho sob rename — as duas saídas coladas no `tasks.md` T6 |
| TYPE-04 | Mesma função | ✅ A mensagem de falha medida cita `cd src-tauri && cargo test --lib types_export -- --ignored`; o mesmo comando está no cabeçalho do arquivo gerado |
| TYPE-05 | `runtime_commands::DownloadableModel` | ✅ com correção: o `ts-rs` 12.0.1 **inlina** os campos, não emite interseção. A saída real está no `design.md`; o rename do T6 falhou apontando um campo do `CuratedModelInfo`, que é o que o requisito pede |
| TYPE-06 | Prova em `tasks.md` (T6) | ✅ Rename de `estimated_ram_gb` → `cargo check` e `npm run build` **limpos**, `cargo test --lib` vermelho na linha 67 do gerado |
| TYPE-07 | `#[ts(type = …)]` em `DocumentRecord.status`, `ChatAttachment.status`, `RuntimeStatus.backend`, `Message.role`; `#[derive(TS)]` em `runtime::Backend` | ✅ Cada um com comentário nomeando o escritor, achado por `grep` — a tabela dos escritores está no `tasks.md` T3. `Backend::as_str()` fica preso à união gerada por `backend_as_str_matches_generated_union` |
| TYPE-08 | `#[ts(type = "number")]` em 9 campos, em 6 structs | ✅ `npm run build` limpo **e** `types_export::tests::no_bigint_reaches_the_frontend`, que é o que cobre o próximo campo de 64 bits |
| TYPE-09 | `generate_types_ts` monta a lista numa macro de ordem fixa | ✅ `types_export::tests::generation_is_deterministic` compara duas execuções; o teste do TYPE-03 compara byte a byte contra o disco |
