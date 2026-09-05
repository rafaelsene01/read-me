# Biblioteca de livros (PDF + Kindle) — Specification

**Milestone:** M10 — Pivô para leitor
**Status:** planejado (2026-09-04). **Nada implementado.**

## Problem Statement

A aba Documentos hoje é uma base de conhecimento para RAG: todo arquivo importado é lido, dividido em chunks, embeddado e indexado no LanceDB para o chat responder a partir dele. O projeto está virando um **leitor** — o usuário quer importar livros (PDF e formatos Kindle), guardá-los numa pasta que ele consiga abrir no explorador de arquivos, e **não** quer nenhum passo de RAG sobre eles. O processamento que esses livros vão receber (extrair o texto, remontar em formato navegável mantendo as gravuras, ler em voz alta com marcação estilo karaokê) é outro trabalho, e depende de decisões que ainda não existem.

Esta feature entrega só a metade que dá para verificar hoje: importar, guardar, listar, remover e abrir a pasta.

## Goals

- [ ] Importar PDF e livros Kindle (`.epub`, `.mobi`, `.azw`, `.azw3`) pela aba que hoje é Documentos
- [ ] Guardar os arquivos em `<base_path>/library/`
- [ ] Botão que abre essa pasta no explorador de arquivos do sistema, com o caminho absoluto visível ao lado
- [ ] **Nenhum passo de RAG** sobre esses arquivos — nem parsing, nem chunking, nem embedding, nem LanceDB
- [ ] Recusar na importação o que o leitor futuro não vai conseguir abrir (formato fora da lista, arquivo com DRM)

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

| Requirement ID | Story | Phase | Status |
| --- | --- | --- | --- |
| LIB-01 | P1: Seletor nativo filtrado aos 5 formatos | in tasks | pending |
| LIB-02 | P1: Copiar para `library/` com sufixo em colisão | in tasks | pending |
| LIB-03 | P1: Recusar extensão fora da lista, sem derrubar a seleção | in tasks | pending |
| LIB-04 | P1: Recusar importação sem pasta-base configurada | in tasks | pending |
| LIB-05 | P1: Recusar `.mobi`/`.azw`/`.azw3` com DRM | in tasks | pending |
| LIB-06 | P1: Recusar `.epub` com DRM | in tasks | pending |
| LIB-07 | P1: Registrar em `books` sem nenhum passo de RAG | in tasks | pending |
| LIB-08 | P1: Não escrever em `documents` | in tasks | pending |
| LIB-09 | P1: Listar com nome, formato e tamanho | in tasks | pending |
| LIB-10 | P1: Remover arquivo e linha, tolerando arquivo ausente | in tasks | pending |
| LIB-11 | P1: Abrir a pasta no explorador do sistema | in tasks | pending |
| LIB-12 | P1: Mostrar o caminho absoluto da pasta na UI | in tasks | pending |

**ID format:** `LIB-[NUMBER]`
