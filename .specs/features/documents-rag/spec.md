# Base de Conhecimento & RAG Global — Specification

> ⛔ **A UI desta feature foi revogada pela feature `book-library` (M10.1), decisão **AD-052** — e a revogação passou de planejada a **executada em 2026-09-05**.** A aba Documentos deixou de existir na navegação: a união `ActiveView` (`src/store/uiStore.ts`) só tem `"library"`, `App.tsx` renderiza o `LibraryPanel`, e `src/components/Sidebar/DocumentsSection.tsx` foi **apagado** (não por escolha: com a união sem `"documents"` o `tsc` falhava nele com `TS2367`/`TS2345`).
>
> **Nenhum requisito foi apagado, de propósito** — o histórico do "porquê" tem valor, e o **backend de RAG continua inteiro**, compilando e coberto pelos mesmos testes. O que deixou de ser verdade é **a porta**: os requisitos de UI (**DOC-01, DOC-02, DOC-03, DOC-05, DOC-08, DOC-09**) não têm mais caminho pela tela. Os de recuperação (**DOC-10, DOC-11, DOC-12**) continuam valendo: o chat não foi revogado nesta rodada e continua consumindo o retrieval.
>
> **O que ficou órfão de rota, presente no repositório e compilando:** `src/components/Documents/DocumentsPanel.tsx`, `DocumentRow.tsx`, `DocumentStatusBadge.tsx`, `src/store/documentsStore.ts`, `src/lib/documentsApi.ts` e as chaves `sidebar.documents` / `documents.*` dos dois arquivos de i18n. Do lado Rust, `document_commands.rs` continua registrado no `invoke_handler` — os comandos existem, ninguém os chama pela UI.
>
> **A remoção física do código não aconteceu e tem gatilho escrito (AD-052):** a primeira sessão depois que o leitor (M10.2) renderizar um livro ponta a ponta. Até lá o binário continua carregando llama.cpp, LanceDB e fastembed, e isso é custo aceito, não esquecido.

## Problem Statement

O usuário quer importar documentos para uma base de conhecimento global e ter o chat respondendo com base neles (RAG). Quando esta feature foi escrita (M5, 2026-07-25), a aba Documentos era só um placeholder — e desde 2026-09-05 ela **não existe mais**, ver o aviso no topo. Esta feature entrega a importação com feedback de progresso — só documentos totalmente processados entram no RAG — e a recuperação (retrieval) que o chat (M4) vai consumir como uma das camadas de contexto.

## Goals

- [x] Importar documentos (PDF, DOCX, TXT, MD) pela aba Documentos — ⛔ **a aba não existe mais** desde 2026-09-05 (AD-052); os comandos continuam registrados, sem caminho pela UI
- [x] Mostrar progresso de processamento por documento (fila → processando → pronto/erro)
- [x] Só documentos com status "pronto" entram na busca RAG
- [x] Buscar trechos relevantes por similaridade e expor isso para o chat injetar no contexto

## Out of Scope

| Feature | Reason |
| --- | --- |
| Anexos dentro de um chat específico | M4 — RAG "de chat" é isolado, esta feature é só a base global |
| OCR de documentos escaneados/imagens | Deferido (ver STATE.md Deferred Ideas) |
| Reindexar tudo ao trocar de modelo de embedding | Deferido — v1 assume o modelo de embedding é fixo por instalação |
| Edição de conteúdo do documento pelo app | Fora de escopo — app só lê/indexa o que foi importado |

---

## User Stories

### P1: Importar documento ⭐ MVP

**User Story**: Como usuário, quero clicar em "importar" na aba Documentos e escolher um arquivo do meu computador, para adicioná-lo à base de conhecimento.

⛔ **Revogado pela `book-library` (AD-052), executado em 2026-09-05.** A aba que abrigava este botão virou a Biblioteca; `import_documents` continua no `invoke_handler` e sem chamador na UI. Requisito mantido por registro histórico — ver o aviso no topo.

**Why P1**: Ação central da feature — sem importar, não há o que indexar.

**Acceptance Criteria**:

1. WHEN o usuário clica em importar THEN o sistema SHALL abrir um seletor de arquivo nativo filtrado para PDF/DOCX/TXT/MD
2. WHEN um ou mais arquivos são escolhidos THEN o sistema SHALL copiá-los para `documents/` (na pasta-base) e registrá-los com status inicial "na fila"
3. WHEN o arquivo é maior que um limite configurável (ex.: 200MB) OU tem extensão não suportada THEN o sistema SHALL rejeitar com mensagem clara antes de copiar

**Independent Test**: Importar um PDF pequeno e ver ele aparecer na lista da aba Documentos imediatamente com status "na fila". ⛔ **Inexecutável desde 2026-09-05** — não há aba nem botão; o teste vale como registro do que foi verificado em 2026-07-27.

---

### P1: Ver progresso de processamento ⭐ MVP

**User Story**: Como usuário, quero ver o progresso de processamento de cada documento importado, para saber quando ele já pode ser usado.

⛔ **Revogado pela `book-library` (AD-052), executado em 2026-09-05** — só a parte de UI. O pipeline de background e os eventos `document-status` continuam existindo; o que saiu foi a tela que os exibia.

**Why P1**: Requisito explícito — só documentos processados devem valer como RAG, então o usuário precisa saber o status.

**Acceptance Criteria**:

1. WHEN um documento está na fila THEN o sistema SHALL processá-lo em background pelas etapas: extrair texto → dividir em chunks → gerar embeddings → indexar
2. WHEN o documento está em qualquer etapa de processamento THEN a UI SHALL mostrar isso visualmente (ex.: "processando", com indicador)
3. WHEN o processamento termina com sucesso THEN o status SHALL virar "pronto" e o documento passa a entrar nas buscas RAG
4. WHEN o processamento falha (arquivo corrompido, texto vazio, erro de parsing) THEN o status SHALL virar "erro" com mensagem, e o documento NÃO SHALL entrar no RAG
5. WHEN vários documentos são importados de uma vez THEN o sistema SHALL processá-los (em paralelo ou fila, à escolha do design) sem travar a UI

**Independent Test**: Importar um documento grande, observar o status mudando de "na fila" → "processando" → "pronto"; importar um arquivo corrompido e ver status "erro".

---

### P1: Listar e remover documentos ⭐ MVP

**User Story**: Como usuário, quero ver todos os documentos importados com seu status, e poder remover os que não quero mais.

⛔ **Revogado pela `book-library` (AD-052), executado em 2026-09-05.** A listagem e a remoção não têm mais tela; `DocumentsPanel.tsx` e `documentsStore.ts` seguem no repositório, órfãos de rota.

**Why P1**: Gestão básica da base — sem isso, a base só cresce e não há como corrigir erros.

**Acceptance Criteria**:

1. WHEN a aba Documentos é aberta THEN o sistema SHALL listar todos os documentos com nome, status e tamanho — ⛔ **sem gatilho desde 2026-09-05**: a aba não existe
2. WHEN o usuário remove um documento THEN o sistema SHALL apagar o arquivo de `documents/`, seus embeddings da tabela global, e o registro
3. WHEN um documento com status "erro" é removido e reimportado THEN o sistema SHALL tentar processar novamente do zero

**Independent Test**: Remover um documento "pronto" e confirmar que uma pergunta que antes recuperava esse trecho não recupera mais.

---

### P2: Retrieval usado pelo chat

**User Story**: Como usuário, quero que minhas perguntas no chat considerem os documentos prontos da base, para receber respostas fundamentadas no meu conteúdo.

**Why P2**: É o "porquê" de tudo isso existir, mas depende do M4 (chat) para ser demonstrável ponta a ponta — esta feature entrega a capacidade de recuperação; o consumo real acontece em M4.

**Acceptance Criteria**:

1. WHEN uma pergunta é feita no chat THEN o sistema SHALL embeddar a pergunta e buscar os top-k trechos mais similares entre os documentos com status "pronto"
2. WHEN nenhum documento está "pronto" THEN a busca RAG global SHALL retornar vazio sem erro (chat funciona normalmente sem contexto extra)
3. WHEN trechos são recuperados e usados THEN o sistema SHALL expor de qual documento cada trecho veio (para citação futura no M4)

**Independent Test**: Com um documento "pronto" contendo um fato específico, fazer uma pergunta relacionada e confirmar que os trechos recuperados vêm daquele documento.

---

## Edge Cases

- WHEN um documento é removido enquanto está "processando" THEN o sistema SHALL cancelar o processamento em andamento e limpar qualquer chunk parcial já indexado
- WHEN dois documentos com o mesmo nome de arquivo são importados THEN o sistema SHALL tratá-los como registros distintos (IDs únicos), sem sobrescrever
- WHEN o texto extraído de um documento está vazio (ex.: PDF só com imagens, sem OCR) THEN o status SHALL virar "erro" com mensagem específica ("nenhum texto encontrado")
- WHEN a pasta-base é trocada (ver feature de Configurações) THEN os documentos já indexados na pasta antiga NÃO SHALL aparecer na nova pasta (consistente com a decisão de não migrar documentos automaticamente)
- WHEN o app é fechado com documentos "na fila"/"processando" THEN ao reabrir THEN o sistema SHALL retomar ou reenfileirar o processamento pendente

---

## Requirement Traceability

| Requirement ID | Story | Phase | Status |
| --- | --- | --- | --- |
| DOC-01 | P1: Importar documento (seletor nativo) | Implemented | ⛔ **UI revogada (AD-052, executada em 2026-09-05)** — sem botão que abra o seletor: a aba virou Biblioteca. A evidência abaixo continua verdadeira sobre 2026-07-27. ✅ **Verificado clicando (2026-07-27)** — o seletor nativo do Windows foi aberto pelo botão e respondido; o arquivo escolhido apareceu na lista em 517 ms |
| DOC-02 | P1: Copiar para pasta + registrar "na fila" | Implemented | ⛔ **UI revogada (AD-052, executada em 2026-09-05)** — o comando existe, ninguém o chama pela UI. Backend intacto. Implemented |
| DOC-03 | P1: Rejeitar arquivo inválido/grande demais | Implemented | ⛔ **UI revogada (AD-052, executada em 2026-09-05)** — idem DOC-02: a recusa continua no backend, sem tela que a mostre. Implemented |
| DOC-04 | P1: Pipeline extrair→chunk→embed→indexar | Implemented | Implemented |
| DOC-05 | P1: UI de progresso por documento | Implemented | ⛔ **UI revogada (AD-052, executada em 2026-09-05)** — a tela que exibia as fases saiu do grafo de rota; os eventos `document-status` continuam sendo emitidos. ✅ **Verificado na tela (2026-07-27)** — a linha do documento passou por `Indexando` (+5,8 s) e chegou a `Pronto` em 16,6 s, num TXT de 134 KB. `Na fila`/`Lendo`/`Dividindo` **não foram capturados**: passam em menos que o intervalo de leitura de 120 ms |
| DOC-06 | P1: Status "erro" com mensagem | Implemented | Implemented |
| DOC-07 | P1: Processar múltiplos sem travar UI | Implemented | Implemented |
| DOC-08 | P1: Listar documentos com status | Implemented | ⛔ **UI revogada (AD-052, executada em 2026-09-05)** — `list_documents` sem chamador na UI. Implemented |
| DOC-09 | P1: Remover documento (arquivo + embeddings) | Implemented | ⛔ **UI revogada (AD-052, executada em 2026-09-05)** — sem botão na tela. ✅ **Verificado clicando (2026-07-27)** pelo lado da lista — o botão Remover tirou o documento da tela. Que os embeddings saem junto continua provado por teste contra LanceDB real, não pela UI |
| DOC-10 | P2: Retrieval top-k por similaridade | Implemented | Implemented (revisto em 2026-07-26 — AD-036: ranqueamento entre namespaces, piso de relevância relativo e expansão para o chunk seguinte). ⚠️ **Revisto de novo em 2026-07-27 pela AD-050**: o pool passou a incluir a memória da conversa, o teto de 4 é compartilhado, e o documento só fica colado na pergunta enquanto for o acerto mais próximo |
| DOC-11 | P2: Retrieval vazio sem erro quando base vazia | Implemented | Implemented |
| DOC-12 | P2: Expor origem/citação dos trechos | Implemented | ✅ **Verificado no app (2026-07-27)** — resposta de um chat com anexo saiu como `3,72 unidades [fonte: relatorio-anexo.txt]`, com o nome do arquivo certo |

**ID format:** `DOC-[NUMBER]`
**Status values:** Pending → In Design → In Tasks → Implementing → Verified → ⛔ Revogado (UI)
**Coverage:** 12 total, 12 implementados; **6 com a UI revogada** (DOC-01, DOC-02, DOC-03, DOC-05, DOC-08, DOC-09) e nenhum apagado. Verificado por teste real: embeddings via ONNX Runtime, isolamento de namespace e deletes no LanceDB. **A importação foi exercitada clicando em 2026-07-27** (DOC-01, DOC-05, DOC-09, DOC-12), o que fecha a pendência que esta linha registrava desde 2026-07-25.

⚠️ **DOC-10 mudou de sentido pela AD-050** — ver a linha na tabela. A medição que motivou a mudança está em `chat::context_assembler::retrieval_quality`: contra a base real do usuário, a pior pergunta que o corpus **responde** fica a 0,3077 e a melhor que ele **não** responde a 0,3150. As duas populações se separam por 0,0073, o que descarta um limiar absoluto e é o motivo de a correção ter sido comparativa em vez de um corte.

---

## Success Criteria

- [x] Importar um PDF/DOCX/TXT/MD real e vê-lo chegar a "pronto" sem intervenção manual
- [x] Um documento "erro" nunca aparece nos resultados de retrieval
- [x] Remover um documento reflete imediatamente na busca (não aparece mais em resultados novos)
- [x] Retrieval retorna trechos com referência de qual documento vieram
