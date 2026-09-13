# AGENTS.md

Instruções para agentes de código trabalhando no ReadMe — um chat de IA desktop que roda o modelo, os embeddings e o banco vetorial inteiramente na máquina do usuário.

Este arquivo é a fonte única. `CLAUDE.md` apenas aponta para cá.

---

## Como este projeto trabalha

Desenvolvimento dirigido por spec, com a estrutura em `.specs/`:

| Onde | O que é |
| --- | --- |
| `project/PROJECT.md` | visão e escopo |
| `project/ROADMAP.md` | milestones e o que cada um entrega |
| `project/STATE.md` | decisões (AD), lições (L), bloqueadores, todos |
| `codebase/` | mapeamento do código existente: stack, arquitetura, convenções, testes, concerns |
| `features/<nome>/` | `spec.md` (requisitos com IDs rastreáveis), `design.md`, `tasks.md` |

Toda funcionalidade tem requisitos com ID (`CHAT-11`, `SELF-06`, `SIDE-04`…). Ao mexer no código que implementa um requisito, atualize a tabela de rastreabilidade da spec correspondente — e atualize com o que é verdade, não com o que se pretendia.

**Para varrer o que falta e executar:** a skill `spec-loop` (`.claude/skills/spec-loop/`) faz isso como orquestradora — reconcilia a documentação contra o código, levanta as decisões que dependem do usuário e pergunta uma a uma, e só então despacha subagents para implementar, com validação adversarial por um agente diferente do que implementou. Ela mantém um journal em `.specs/runs/` que permite retomar numa sessão nova.

---

## A regra mais importante: "compila" não é "verificado"

Este repositório separa deliberadamente as duas coisas, e a separação já pagou várias vezes. Casos reais registrados:

- Um `ci.yml` foi dado como pronto com a evidência "YAML validado". Na primeira execução real ele falhou (L-005).
- Seis requisitos foram marcados como implementados quando só o backend existia; a UI não fechava o ciclo (AD-027).
- Um teste de "não há janela de console" passou por um motivo errado e teria sido registrado como prova se não tivesse sido questionado (AD-041).

Portanto, ao relatar trabalho:

- diga **o que foi executado**, não o que deveria funcionar;
- se algo não foi exercitado, diga isso com todas as letras, na mesma frase em que descreve o que foi feito;
- prefira uma evidência medida (um número, uma saída de comando) a um adjetivo.

Quando um teste automatizado não conseguir provar algo, escreva **dentro do teste** por que ele é inconclusivo, para ninguém depois o ler como prova.

---

## Comandos

```bash
# Backend: a suíte inteira (o gate padrão)
cd src-tauri && cargo test --lib

# Backend: só compilar
cd src-tauri && cargo check --lib

# Frontend: tsc + Vite
npm run build

# Frontend: a suíte de testes — CONFIGURADA, VAZIA. `npm test` sai com
# "No test files found" (exit 1): não há um único *.test.ts(x) nesta árvore,
# e o vitest.config.ts aponta para src/test/setup.ts e dois dobles que não existem.
npm test

# Scripts de release (Node puro)
npm run test:scripts

# NÃO EXISTE nesta árvore: src-tauri/src/types_export.rs não está em commit nenhum.
# src/types.ts é escrito à MÃO. O comando abaixo falha; está aqui só para
# ninguém procurá-lo de novo (medido em 2026-09-05, AD-054).
# cd src-tauri && cargo test --lib types_export -- --ignored

# Rodar o app de verdade
npm run tauri dev
```

`cargo test --lib` está em **339 passando / 0 falhas / 22 ignorados** (medido em 2026-09-12; +5 com a tabela `readings`, AD-071: `readings_is_migration_eleven`, `a_database_stopped_at_ten_keeps_every_saved_position_as_a_reading`, `deleting_a_book_deletes_its_readings`, `reading_a_book_again_is_a_new_entry_at_the_first_page` e `starting_a_reading_of_a_book_that_does_not_exist_is_an_error`; os testes de histórico existentes foram **reescritos sobre leituras, nenhum apagado**). Antes, no mesmo dia: **334 / 0 / 22**, +2 com `each_spine_document_is_its_own_chapter_in_spine_order` e `every_chapter_opens_a_page_and_a_long_one_still_spans_several` (AD-068). Antes, no mesmo dia: **332 / 0 / 22**, +1 com `the_cover_is_the_image_the_package_declares_and_nothing_is_guessed` (AD-067). Antes: **331 / 0 / 22** em 2026-09-08, depois de a importação passar a aceitar só EPUB (AD-065). **A queda de 335 para 331 é intencional e tem nome:** saíram `a_palmdb_without_encryption_passes`, `a_palmdb_with_a_non_zero_encryption_field_is_refused`, `a_truncated_palmdb_is_a_read_error_not_a_clean_file` — o `palmdb_has_drm` que os três testavam foi apagado com os formatos `.mobi`/`.azw`/`.azw3` — e `a_pdf_is_never_inspected`, que fixava uma decisão de escopo sobre um formato que não é mais importável. Antes disso: **335 / 0 / 22** pela AD-064, **334 / 0 / 22** pela AD-063 e **333 / 0 / 21** pela AD-062, todas no mesmo dia; **294 / 0 / 18** em 2026-09-07 pela `epub-fidelity`; **265 / 0 / 17** em 2026-09-06 pela AD-057; e **195 / 0 / 15** no fim do M10.1. Os dois ignorados que exigem recurso real são `rag::pdfium::tests::extracting_two_pdfs_in_the_same_process_reuses_the_bindings` e `rag::pdfium::tests::illustrations_of_a_real_pdf_are_extracted_and_marked_in_the_text`, ambos apontados por variável de ambiente para a biblioteca pdfium e um PDF de verdade. **O frontend não tem suíte:** `npm test` sai com *"No test files found"* (exit 1) — zero testes em zero arquivos. Os números anteriores desta linha (181/0/16 e 63 testes em 8 arquivos, de 2026-07-28) foram **medidos e desmentidos** em 2026-09-05: o baseline real antes do M10.1 era **177 / 0 / 15**. `npm run test:scripts` está em **49** e esse batia. Se qualquer um dos números cair, cada teste perdido precisa de justificativa — remoção legítima (o código que ele testava saiu) é aceitável; deleção silenciosa não.

Este número é baseline, então mantenha-o medido: ele ficou parado em 146 por várias sessões enquanto a suíte crescia, e um baseline defasado não detecta perda nenhuma — que é exatamente o que ele existe para fazer.

**Componentes que viajam no instalador** (`npm run vendor`, `scripts/vendor.json`): llama.cpp
Vulkan + CPU, ONNX Runtime, pdfium e **piper** (voz). Medido em 2026-09-07: `resources/` em
**184,7 MB** no Windows, dos quais o piper são **28,6 MB** já sem o `libtashkeel_model.ort`
(10.261.536 bytes de árabe, podado por nome). **As vozes não estão aí** — são modelos, baixadas sob
demanda para a pasta-base, como os GGUF.

**Pré-requisito de build que não é óbvio:** o `lancedb` exige o compilador **protoc**. Sem ele o `cargo build` falha com *"Could not find `protoc`"*. Windows: `winget install Google.Protobuf`. Linux: `apt install protobuf-compiler`.

**Node 22+** é obrigatório: o `npm run test:scripts` depende de expansão de glob que versões anteriores não têm.

---

## Testes: o que exige cobertura

Da matriz em `.specs/codebase/TESTING.md`:

| Camada | Tipo de teste |
| --- | --- |
| Funções puras Rust (parsing, chunking, montagem de contexto, fórmulas) | unit — obrigatório |
| Comandos Tauri que só orquestram I/O | nenhum (não há runner de integração Tauri) |
| Componentes React | nenhum (não há Vitest/RTL) |
| Scripts Node | unit via `node --test` |

`#[cfg(test)] mod tests` fica **no fim do mesmo arquivo**, nunca num diretório `tests/` separado.

Quando algo só pode ser provado contra um recurso real, use `#[ignore]` — o teste fica no repositório, documentado e repetível, sem pesar na suíte padrão. Há dois formatos em uso:

- **recurso que o teste cria**, como um LanceDB numa pasta temporária: `rag::store`;
- **recurso que já existe na máquina** (o binário do llama.cpp, um banco de verdade): o caminho vem de **variável de ambiente** e nunca é adivinhado — `db::real_database`, `runtime::detect::detect_real`, `runtime::process::sidecar_real`.

O segundo formato é o que impede um teste de encostar por acidente nos dados do usuário.

---

## Nunca faça

**Não toque nos dados reais do usuário.** A pasta-base fica fora do repositório e contém as conversas dele. Para validar uma migração, **copie o banco** para o scratchpad, migre a cópia e apague. O original nunca é aberto para escrita por um teste.

**Não deixe arquivo de diagnóstico temporário no repositório.** Já havia um (`rag/diag.rs`) com caminhos absolutos da máquina do usuário, órfão de qualquer `mod`, meses depois de a investigação ter terminado. Se criar um, apague antes de terminar.

**Não faça force-push nem reescreva `master`.** Desfazer é `git revert`, mesmo que fique feio no histórico.

**Não commite sem o usuário pedir.** O padrão é deixar as mudanças no working tree.

**Não dispare release.** O workflow é `workflow_dispatch` manual, de propósito: nenhum push publica nada.

---

## Banco de dados

Migrações versionadas por `PRAGMA user_version`, numa lista ordenada em `db.rs`, cada uma em transação. **A próxima é a 12** — a lista `MIGRATIONS` termina hoje na 11 (`MIGRATION_11_READINGS`, a tabela `readings` com uma linha por leitura do histórico; a 10 é `MIGRATION_10_BOOK_READER`, as colunas de leitura em `books`).

Confira o número na lista antes de escrever a migração, nunca aqui: esta linha já esteve errada, apontando a 8 depois de ela ter sido gasta. Duas migrações com o mesmo número não colidem em compilação — a segunda simplesmente nunca roda, porque o `user_version` já passou dela.

`CREATE TABLE IF NOT EXISTS` sozinho não migra banco existente — vira no-op silencioso. Mudança de coluna exige entrada nova na lista.

**Chaves estrangeiras são aplicadas** (`PRAGMA foreign_keys = ON` no `open`). Isso muda a ordem de operações destrutivas: derrube a tabela que referencia antes da referenciada, senão falha.

Toda migração destrutiva deve ser ensaiada contra uma cópia de um banco real antes de ser considerada pronta.

---

## Convenções de código

Detalhe completo em `.specs/codebase/CONVENTIONS.md`. O essencial:

- **Comentários em inglês, no código.** Explicam **por quê**, não o quê — de preferência ancorados numa medição ou num caso real ("verified live against llama-server: n_ctx_train = 131072"). Comentário que repete o nome da função é ruído.
- **Prosa de documentação em português** (README, `.specs/`, este arquivo). Código, nomes e mensagens de commit em inglês.
- **Commits em Conventional Commits**, em inglês, um por task. O CI valida isso em PRs.
- Arquivos Rust `snake_case.rs`; comandos Tauri sempre em arquivo com sufixo `_commands`.
- Componentes React `PascalCase.tsx`, um por arquivo. Wrappers de `invoke` em `*Api.ts`, stores Zustand em `*Store.ts`.
- Campos que cruzam a fronteira Rust↔TS são `snake_case` dos dois lados (o serde não renomeia) — `src/types.ts` quebra o camelCase de propósito. A exceção são os **parâmetros** de `invoke()`, que vão camelCase e chegam snake_case; o Tauri converte.
- `src/types.ts` é **escrito à mão**, e **não há gate nenhum sobre ele**. Isto contradiz o que esta linha dizia até 2026-09-05 ("gerado desde 2026-07-28 pela feature `generated-types`"): `src-tauri/src/types_export.rs` **não existe em commit algum** deste repositório, e portanto o gate `types_export::tests::types_ts_matches_rust_structs` também não. Mudou uma struct que cruza a fronteira? **Confira os campos um a um** — uma divergência de tipo deixa `cargo check` e `npm run build` os dois limpos e ninguém avisa. A feature `generated-types` continua documentada em `.specs/features/`, e é spec sem código (AD-054).

---

## Armadilhas conhecidas desta base

**i18n tem paridade obrigatória.** `en.json` e `pt.json` precisam ter exatamente as mesmas chaves. Adicionou uma, adicione nos dois.

**Streaming não passa pelo retorno do comando.** Comandos Tauri são request/response; tokens chegam por evento (`chat-stream-chunk`). Mesmo padrão para progresso de download e status de indexação.

**Um modelo pequeno responde ao que está perto da pergunta.** Os trechos recuperados entram no mesmo turno da pergunta, não num bloco `system` no topo — mudar isso fez o modelo parar de copiar as próprias respostas anteriores (AD-033).

**O orçamento do prompt reserva o que a resposta vai usar.** Se mexer em `answer_token_budget`, mexa no `budget_chars` junto: eles se referem ao mesmo espaço.

**Turnos precisam alternar.** Uma geração cancelada deixa a pergunta sem resposta, e dois `user` seguidos fazem o modelo divagar em vez de responder.

**O sidecar é morto por Job Object no Windows**, além do `kill` explícito. Se mexer no spawn, não remova nenhum dos dois: um cobre o fechamento normal, o outro cobre o kill forçado.
