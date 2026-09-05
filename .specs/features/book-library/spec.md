# Biblioteca de livros (PDF + Kindle) — Specification

**Milestone:** M10 — Pivô para leitor
**Status:** **implementado em 2026-09-05 (T1–T8 de 9), não verificado no app.** Os gates passaram — `cargo test --lib` **195 passando / 0 falhas / 15 ignorados** (baseline da T1: 177/0/15), `cargo check --lib` **zero warnings**, `npm run build` exit 0 com o bundle mudando para `index-BhmqRmEJ.js`, i18n **158/158 chaves**. **`npm run tauri dev` não rodou uma única vez e nenhum `invoke` foi disparado.** Por isso **nenhum requisito abaixo está `Verified`** — marcá-los seria repetir o erro registrado na AD-027. A T9 é a UAT que fecha isso.

## Problem Statement

A aba Documentos **era**, até esta feature entrar (2026-09-05), uma base de conhecimento para RAG: todo arquivo importado é lido, dividido em chunks, embeddado e indexado no LanceDB para o chat responder a partir dele. O projeto está virando um **leitor** — o usuário quer importar livros (PDF e formatos Kindle), guardá-los numa pasta que ele consiga abrir no explorador de arquivos, e **não** quer nenhum passo de RAG sobre eles. O processamento que esses livros vão receber (extrair o texto, remontar em formato navegável mantendo as gravuras, ler em voz alta com marcação estilo karaokê) é outro trabalho, e depende de decisões que ainda não existem.

Esta feature entrega só a metade que dá para verificar hoje: importar, guardar, listar, remover e abrir a pasta.

## Goals

- [x] Importar PDF e livros Kindle (`.epub`, `.mobi`, `.azw`, `.azw3`) pela aba que era Documentos
- [x] Guardar os arquivos em `<base_path>/library/`
- [x] Botão que abre essa pasta no explorador de arquivos do sistema, com o caminho absoluto visível ao lado
- [x] **Nenhum passo de RAG** sobre esses arquivos — nem parsing, nem chunking, nem embedding, nem LanceDB
- [x] Recusar na importação o que o leitor futuro não vai conseguir abrir (formato fora da lista, arquivo com DRM)

⚠️ **O que estes `[x]` significam e o que não significam:** o código existe, compila e o que é função pura está coberto por teste. **Nenhum deles foi exercitado no app** — os três primeiros dependem de `library_dir()`, que exige um `AppHandle` e nunca rodou. Ver a coluna de evidência da tabela de rastreabilidade.

## Out of Scope

| Feature | Reason |
| --- | --- |
| Extrair o texto e remontar o livro em formato navegável | É o leitor (M10.2); depende de decidir a âncora de posição e como preservar as gravuras |
| Renderizar o livro dentro do app | Idem — sem renderizador, não há o que exibir |
| Leitura em voz alta (audiobook) e marcação karaokê | M10.3; **bloqueado por uma viabilidade não medida**: exige TTS local com limite por palavra (word boundary), que é raro |
| Histórico de leitura e "onde parou" | `.specs/features/reading-history/spec.md` — a posição não tem escritor enquanto não existir leitor |
| Remover o código de chat e de RAG | Revogação registrada na AD-052; a remoção física é trabalho próprio, com gatilho escrito |
| `.kfx` | Nenhuma biblioteca aberta lê KFX de forma confiável; aceitar no seletor seria prometer o que o leitor não vai cumprir |
| Detecção de PDF protegido por senha | Não há parsing nesta feature; a falha aparece no leitor |
| Deduplicação por hash de conteúdo | O sufixo `(2)` que já existe resolve colisão de nome; hash é código a mais sem caso relatado |

---

## Assumptions & Open Questions

| Assunção | Escolha adotada | Racional |
| --- | --- | --- |
| "Pasta onde o programa está instalado" | `<base_path>/library/` | No modo **portátil** o `base_path` já é `./data` ao lado do executável (AD-034), ou seja, o pedido literal está atendido. No modo **instalado**, gravar em `C:\Program Files\…` exige administrador e o M8 tem como requisito instalar **sem** administrador; escrever lá quebraria o produto. O que resolve a dor real — achar os arquivos — é o botão de abrir a pasta mais o caminho visível (LIB-11, LIB-12). |
| Quais são "os formatos Kindle" | `.epub`, `.mobi`, `.azw`, `.azw3` | Kindle não é um formato, são quatro famílias. `.kfx` fica fora por falta de leitor aberto. `.epub` entra porque o próprio Kindle passou a aceitá-lo e é o formato que o leitor futuro tem mais chance de remontar. |
| Livro com DRM | Recusado na importação, com mensagem dizendo que está protegido | A maioria dos arquivos Kindle de uma pessoa é compra da Amazon, e portanto tem DRM. Aceitar agora e falhar só no leitor encheria a biblioteca de livros que nunca abrem — e a culpa cairia no leitor, não na importação. |
| Limite de tamanho | Nenhum | Os 100 MB de hoje existem porque embeddar um arquivo enorme leva minutos (`MAX_FILE_BYTES` em `document_commands.rs`). Sem RAG sobra só um `fs::copy`, e um PDF digitalizado de arte passa de 100 MB legitimamente. |
| Onde o estado mora | Tabela **nova** `books`, migração **9** | Reusar `documents` foi descartado por evidência: `discard_interrupted_attachments` roda no boot (`lib.rs:111`) e executa `DELETE FROM documents WHERE namespace <> 'global'` — uma linha de livro fora do namespace `global` **seria apagada a cada abertura do app**; dentro de `global` ela apareceria na lista de RAG (`SELECT_DOCUMENT`). Reusar exigiria mexer em 5 constantes SQL, no pipeline e nos testes deles; tabela nova não toca em nada disso. |
| Caminho gravado no banco | Só o `filename`; o caminho é resolvido como `<base_path>/library/<filename>` | Caminho absoluto quebra quando o portátil muda de letra de drive. |
| A aba Documentos passa a ser a Biblioteca | Sim — a UI de importação para RAG sai | Foi o que o usuário pediu ("na parte que temos de documentos"). A revogação está registrada na AD-052 e anotada em `documents-rag/spec.md`; o backend de RAG **continua no lugar** nesta feature. |
| `src/types.ts` é gerado ou escrito à mão | Decidir ao executar, conferindo se `src-tauri/src/types_export.rs` existe na árvore restaurada | O `AGENTS.md` afirma que o arquivo é gerado desde 2026-07-28, mas **o `HEAD` não tem o módulo `types_export`** e o `package.json` do `HEAD` não tem o script `test`. A feature `generated-types` está documentada e não commitada. |

Open questions: nenhuma bloqueia esta feature. A única em aberto no milestone — o karaokê exige TTS local com limite por palavra. Isso não foi medido, e é o risco que decide se o produto inteiro é viável. Registrado na AD-052 como gate do M10.3.

---

## User Stories

### P1: Importar livro ⭐ MVP

**User Story**: Como leitor, quero escolher arquivos PDF ou Kindle do meu computador e trazê-los para dentro do app, para montar minha biblioteca.

**Acceptance Criteria**:

1. WHEN o usuário aciona a importação THEN o sistema SHALL abrir um seletor de arquivo nativo filtrado para `pdf`, `epub`, `mobi`, `azw` e `azw3`
2. WHEN um ou mais arquivos são escolhidos THEN o sistema SHALL copiar cada um para `<base_path>/library/`, preservando o nome original e acrescentando um sufixo numérico em caso de colisão
3. IF um arquivo escolhido tem extensão fora da lista WHEN a importação roda THEN o sistema SHALL recusar aquele arquivo pelo nome, com o motivo, e SHALL importar normalmente os demais da mesma seleção
4. IF a pasta-base ainda não está configurada WHEN o usuário importa THEN o sistema SHALL recusar a operação com a mensagem de armazenamento não configurado, sem copiar nada

**Independent Test**: escolher, na mesma seleção, um `.epub` válido e um `.docx`; o EPUB aparece na lista e o DOCX aparece recusado pelo nome.

---

### P1: Recusar livro protegido ⭐ MVP

**User Story**: Como leitor, quero ser avisado na hora da importação que um livro está protegido por DRM, para não descobrir isso só quando tentar abri-lo.

**Acceptance Criteria**:

1. WHEN um arquivo `.mobi`, `.azw` ou `.azw3` tem o campo de criptografia do registro 0 do PalmDB diferente de zero THEN o sistema SHALL recusá-lo antes de copiar, informando que está protegido
2. WHEN um arquivo `.epub` contém `META-INF/encryption.xml` THEN o sistema SHALL recusá-lo antes de copiar, informando que está protegido
3. IF o arquivo não pode ser lido para essa verificação WHEN a importação roda THEN o sistema SHALL recusá-lo com o erro de leitura, sem copiá-lo

**Independent Test**: montar um `.mobi` mínimo com o campo de criptografia em `1` e confirmar a recusa; o mesmo arquivo com `0` importa.

---

### P1: A importação não indexa nada ⭐ MVP

**User Story**: Como leitor, quero que importar um livro seja instantâneo e não gaste CPU indexando, porque não vou fazer perguntas sobre ele.

**Acceptance Criteria**:

1. WHEN um livro é importado THEN o sistema SHALL registrá-lo na tabela `books` e SHALL NOT executar extração de texto, chunking, embedding ou escrita no LanceDB
2. WHEN um livro é importado THEN o sistema SHALL NOT inserir linha na tabela `documents`

**Independent Test**: importar um PDF e confirmar, no banco, que `books` tem uma linha e `documents` tem zero linhas novas.

---

### P1: Listar e remover ⭐ MVP

**User Story**: Como leitor, quero ver os livros que importei e poder tirar os que não quero mais.

**Acceptance Criteria**:

1. WHEN a Biblioteca é aberta THEN o sistema SHALL listar cada livro com nome, formato e tamanho, do mais recente para o mais antigo
2. WHEN o usuário remove um livro THEN o sistema SHALL apagar o arquivo de `library/` e a linha correspondente
3. IF o arquivo já não existe no disco WHEN o usuário remove o livro THEN o sistema SHALL apagar a linha mesmo assim, sem erro

**Independent Test**: importar dois livros, remover um, reabrir o app e confirmar que sobrou um, e que o arquivo removido saiu da pasta.

---

### P1: Abrir a pasta da biblioteca ⭐ MVP

**User Story**: Como leitor, quero um botão que abra a pasta onde os livros estão guardados, para mexer neles fora do app.

**Acceptance Criteria**:

1. WHEN o usuário aciona "abrir pasta" THEN o sistema SHALL abrir `<base_path>/library/` no explorador de arquivos do sistema
2. WHEN a Biblioteca é exibida THEN a UI SHALL mostrar o caminho absoluto da pasta ao lado do botão
3. IF a pasta ainda não existe WHEN qualquer operação da Biblioteca roda THEN o sistema SHALL criá-la antes de continuar
4. WHERE a instalação é portátil THEN a pasta SHALL ficar sob `./data/library`, ao lado do executável

**Independent Test**: num app recém-instalado, sem nenhum livro importado, clicar em "abrir pasta" e ver o explorador abrir numa pasta `library` vazia.

---

## Edge Cases

- WHEN dois arquivos com o mesmo nome são importados THEN o segundo SHALL virar `nome (2).ext`, sem sobrescrever o primeiro
- WHEN a pasta-base é trocada nas Configurações THEN os livros da pasta antiga SHALL NOT aparecer na nova (mesmo comportamento já decidido para documentos)
- WHEN o usuário apaga um arquivo direto na pasta, pelo explorador THEN a lista SHALL continuar mostrando a linha até que ele a remova pelo app — a reconciliação disco↔banco fica para o leitor, que é quem precisa abrir o arquivo
- WHEN a mesma seleção mistura arquivos válidos, inválidos e com DRM THEN cada um SHALL ser julgado por si, e os válidos SHALL entrar

---

## Requirement Traceability

| Requirement ID | Story | Phase | Evidência medida — e o que falta |
| --- | --- | --- | --- |
| LIB-01 | P1: Seletor nativo filtrado aos 5 formatos | **Implemented** (T6) | `open()` do `@tauri-apps/plugin-dialog` com `extensions: ["pdf","epub","mobi","azw","azw3"]`, compilado por `tsc` (`npm run build` exit 0). **Nunca aberto:** o seletor é UI do SO e só existe com o app rodando — T9 |
| LIB-02 | P1: Copiar para `library/` com sufixo em colisão | **Implemented** (T4) | unit `a_second_book_with_the_same_name_gets_a_suffix_instead_of_overwriting`: `livro.pdf` mantém o conteúdo do primeiro, `livro (2).pdf` recebe o segundo, e a linha grava **o nome de destino**. `unique_destination` foi **reusada** de `document_commands.rs`, não reescrita. **Falta:** o comando `import_books` em si nunca rodou |
| LIB-03 | P1: Recusar extensão fora da lista, sem derrubar a seleção | **Implemented** (T3, T4, T6) | units `the_five_book_formats_are_accepted`, `other_formats_are_refused` (`.docx`, `.kfx`, `.txt`, `.md`, sem extensão) e `a_mixed_selection_keeps_the_valid_files_and_names_the_refused_ones` — o válido entra, os dois recusados são nomeados e **não tocam a pasta**. O painel renderiza `rejected.map(...)` com nome + motivo. **Falta:** essa lista nunca foi vista na tela |
| LIB-04 | P1: Recusar importação sem pasta-base configurada | **Implemented, NÃO MEDIDO** (T4) | Escrito: o erro vem do `load_config` através de `library_dir()`. **Zero prova de execução** — `library_dir()` exige um `AppHandle` e **nunca rodou**, em teste nenhum. É a T9 |
| LIB-05 | P1: Recusar `.mobi`/`.azw`/`.azw3` com DRM | **Implemented** (T3) | units com PalmDB sintético: campo de criptografia em **0** → `Ok(false)`; em **1** e **2** → `Ok(true)`; truncado / offset inválido / inexistente → **`Err`**, nunca "limpo". Essa última asserção pegou um defeito real durante a T3 (86 bytes zerados eram relatados como sem DRM) e continua no teste. **Falta:** nenhum `.mobi`/`.azw`/`.azw3` **real** passou por aqui — os offsets batem com o formato documentado, não com um arquivo produzido por um Kindle |
| LIB-06 | P1: Recusar `.epub` com DRM | **Implemented** (T3) | units com zip montado pelo crate `zip`: com `META-INF/encryption.xml` → `Ok(true)`; sem → `Ok(false)`; arquivo que não abre como zip → `Err`. **Falta:** nenhum EPUB real, com ou sem DRM |
| LIB-07 | P1: Registrar em `books` sem nenhum passo de RAG | **Implemented** (T2, T4) | migração **9** conferida na lista antes de escrever; units `books_is_migration_nine`, `a_fresh_database_gets_the_books_table_at_version_nine`, `a_database_stopped_at_eight_upgrades_to_nine_keeping_its_rows` e `importing_a_book_writes_to_books_and_never_to_documents` (`COUNT(*) FROM books = 1`). **Falta:** a migração **não foi ensaiada contra cópia de banco real** (`db::real_database` continua `#[ignore]`) |
| LIB-08 | P1: Não escrever em `documents` | **Implemented** (T4) | mesmo teste acima, na asserção `COUNT(*) FROM documents = 0` depois de importar. É a prova mais direta de "sem RAG" que existe sem abrir o app |
| LIB-09 | P1: Listar com nome, formato e tamanho | **Implemented** (T4, T6, T7) | unit `books_are_listed_from_the_newest_to_the_oldest` (`imported_at DESC` vindo do SQL; o store **não** reordena, para não criar segunda fonte de verdade). **Falta:** a lista nunca foi renderizada — nem em teste (não há suíte de frontend na árvore) nem no app |
| LIB-10 | P1: Remover arquivo e linha, tolerando arquivo ausente | **Implemented** (T4) | units `removing_a_book_deletes_the_file_too` e `removing_a_book_whose_file_is_already_gone_still_drops_the_row` (→ `Ok(())`, linha some). **Falta:** o botão nunca foi clicado |
| LIB-11 | P1: Abrir a pasta no explorador do sistema | **Implemented, NÃO MEDIDO** (T4, T6) | Escrito: `openPath(libraryPath)` do `@tauri-apps/plugin-opener`, desabilitado enquanto o caminho é `null`; `library_dir()` faz `create_dir_all` e é o único caminho de acesso à pasta, o que faz **LIB-11.3** valer também para a biblioteca vazia. **LIB-11.3 e LIB-11.4 (modo portátil) estão escritos, não medidos** — `library_dir()` nunca rodou. É a T9 |
| LIB-12 | P1: Mostrar o caminho absoluto da pasta na UI | **Implemented, NÃO MEDIDO** (T5, T6) | Escrito: `libraryPath: string \| null` com ação própria `loadLibraryPath`, renderizado ao lado dos botões — não atrás de um clique. **Falta tudo o que importa aqui:** a tela nunca montou, então não se sabe se o caminho aparece nem se cabe na linha |

**ID format:** `LIB-[NUMBER]`
**Status values:** Pending → In Design → In Tasks → **Implemented** → Verified
**Coverage:** 12 total. **12 `Implemented`, 0 `Verified`** — a diferença é deliberada e é a regra do `AGENTS.md`: "compila" não é "verificado". Prova automatizada: **18 testes novos em Rust** (3 na migração, 9 na detecção de formato/DRM, 6 nos comandos), com `cargo test --lib` em **195 / 0 falhas / 15 ignorados**. **Sem prova de execução:** o app não foi aberto, nenhum `invoke` foi disparado, os 4 comandos Tauri não têm teste (não há runner de integração Tauri), `library_dir()` nunca rodou, nenhum livro real foi importado e **não existe teste de frontend nesta árvore**. Tudo isso é a **T9**.

⚠️ **Uma armadilha específica desta feature:** `src/types.ts` foi escrito **à mão**, porque o gerador que o `AGENTS.md` descreve (`src-tauri/src/types_export.rs`) **não existe nesta árvore** — medido na T1. Consequência: uma divergência entre `BookRecord` no Rust e a interface TS passa por `cargo check` **e** por `npm run build` os dois limpos, sem nada acusar. Os cinco campos foram conferidos um a um na T5, e essa conferência é humana, não um gate.
