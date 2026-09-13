# Leitor de livros — processar, traduzir, ler — Specification

**Milestone:** M10.2 — Leitor
**Status:** planejado em 2026-09-05. **Nenhuma linha de código foi escrita.** Todas as afirmações sobre o código existente abaixo foram lidas do repositório nesta sessão e estão citadas com arquivo e linha; nenhuma foi executada.

## Problem Statement

O usuário pediu, em uma frase só, o ciclo inteiro do leitor:

> "quando importar quero que ele fique com o nome do arquivo mostrando na lista e me dê opção de processar o arquivo, esse processamento deve me dar a opção de qual idioma ele vai ser processado, e basicamente ele deve extrair o texto do documento e traduzir se necessário e remontar os dados de tal forma que possamos mostrar para leitura como se fosse um livro e o que temos de chat vai ser histórico de leitura, pois deve deixar salvo a página em que estamos na leitura. Para abrir essa leitura, vamos na biblioteca e selecionamos um dos livros/documentos para leitura e ele já deve abrir em uma sessão com histórico."

A `book-library` (M10.1) entregou a metade de baixo: importar, guardar, listar, remover, abrir a pasta. O arquivo entra em `<base_path>/library/` e vira uma linha em `books`, **e nada mais acontece com ele** — não há extração de texto, não há paginação, não há leitor. A `reading-history` tem os requisitos escritos desde 2026-09-04 e está **explicitamente bloqueada** por não existir quem escreva a posição de leitura, e por "posição" não ter significado enquanto o leitor não decidir como o livro é remontado.

Esta feature entrega o meio que falta e, ao entregá-lo, **desbloqueia a `reading-history`**: a âncora de posição passa a existir e é decidida aqui.

## Goals

- [ ] Cada livro na Biblioteca mostra o nome do arquivo e o **estado de processamento**, com um botão para processar
- [ ] Processar pergunta o **idioma de leitura** antes de começar
- [ ] Extrair o texto do arquivo e **remontá-lo em páginas** persistidas
- [ ] **Traduzir** para o idioma escolhido, **pelo modelo local via llama.cpp**, com um modelo default designado, **um parágrafo por requisição**
- [ ] Ler na tela como um livro: uma página por vez, avançar e voltar
- [ ] A lista lateral deixa de listar conversas e passa a listar **leituras**, com a posição salva
- [ ] Selecionar um livro na Biblioteca abre a leitura **na página em que parou**
- [ ] **Editar** um livro já processado: trocar o modelo e **reprocessar só as páginas escolhidas**
- [ ] Um mesmo livro guarda **traduções em vários idiomas ao mesmo tempo**, e alternar entre elas é instantâneo
- [ ] O texto processado é gravado em **arquivos, uma pasta por idioma, dentro da pasta do livro**

## Out of Scope

| Feature | Reason |
| --- | --- |
| Imagens e gravuras no texto remontado | ⛔ **REVOGADO em 2026-09-07 pelo usuário** (AD-058), exatamente pelo mecanismo que esta linha previa: ela estava registrada **como suposição a vetar**, e foi vetada. As gravuras passam a ser escopo da feature `book-illustrations` (ILLUS-01 a ILLUS-12), que estende esta. Texto original, mantido por registro histórico: *o pedido do usuário diz "extrair o texto" e não menciona imagem; o `ROADMAP.md` do M10.2 fala em "preservando as gravuras" — as duas fontes divergem, e esta spec segue o pedido*. A divergência foi resolvida a favor do `ROADMAP.md` |
| Leitura de `.mobi`, `.azw`, `.azw3` | Não existe crate Rust vetado por este projeto que leia PalmDB/Mobipocket. Escrever um parser binário do zero não cabe nesta fatia. Os arquivos continuam **importáveis** (LIB-01 vale) e a linha diz, na lista, que a leitura não é suportada — READ-03 |
| Leitura em voz alta e karaokê | M10.3, e continua com a viabilidade de TTS por palavra **não medida** (AD-052) |
| Remover o código de chat e de RAG | **Esta feature dispara o gatilho** escrito na AD-052 item 4 ("a primeira sessão depois que o leitor renderizar um livro ponta a ponta"). Ela não executa a remoção; ela a torna devida — READ-19 |
| Sumário / índice de capítulos navegável | O spine do EPUB é lido para **ordenar** o texto, não para gerar navegação. Índice é feature própria |
| Busca dentro do livro | Ninguém pediu |
| Marcadores, anotações, destaques | Ninguém pediu |
| Ajuste de fonte, margens, tema de leitura | Ninguém pediu; o tema do app já se aplica |
| Manter o layout `library/<arquivo>` da M10.1 | **Revogado pela READ-32**, a pedido do usuário: o livro passa a morar em pasta própria. `book-library/spec.md` LIB-02 foi **anotado, não apagado** (feito pela T17 em 2026-09-06, no critério 2 da story "Importar livro" e na linha da tabela) |
| Reancorar a posição salva quando o extrator mudar de versão | A `reading-history` lista isso como edge case. A escolha aqui é **invalidar explicitamente** (clampar), nunca deslocar em silêncio — READ-13 |

---

## Assumptions & Open Questions

Quatro dessas escolhas passaram por council de quatro vozes (Arquiteto, Cético, Pragmático, Crítico). Em duas delas o council votou 3×0 contra a posição inicial do Arquiteto; uma foi aceita, a outra foi rejeitada **por evidência do próprio repositório**, e o motivo está dito.

| Assunção | Escolha adotada | Racional |
| --- | --- | --- |
| **Traduzir com o modelo local é viável?** | **Não se sabe, e por isso a T1 mede antes de a T6 existir.** A tradução **fica no escopo**, opt-in, com o custo medido e mostrado na tela antes de começar | O council votou 3×0 por **cortar a tradução desta fatia**. A objeção real das três vozes não era "não faça", era **"ninguém mediu se o Phi-3.5 3.8B Q4_K_M traduz de forma utilizável, nem a que velocidade"** — e isso é verdade. Cortar seria estreitar o pedido do usuário por uma suposição não medida, que é o erro oposto ao que o `AGENTS.md` combate. A síntese: a medição vira a **primeira task**, com regra de parada escrita, e a tradução só é construída depois que o número existir |
| **Quando a tradução roda** | Livro inteiro, **sequencial, retomável**, durante o processamento, com progresso e cancelamento. A unidade de **gravação** é a página; a unidade de **requisição** é o **parágrafo** | Tradução preguiçosa por página (a alternativa) trava a virada de página no meio da leitura e ainda exige cache por página, por idioma e por modelo — o Crítico mediu isso como **mais** superfície de bug, não menos. Retomável não é refinamento: se o app fechar no meio de um livro de 300 páginas, a página já traduzida não pode ser perdida. Como as páginas já são linhas, o checkpoint sai de graça |
| **A unidade de requisição é o parágrafo** | **Um parágrafo por requisição.** Nunca uma página inteira num prompt só — instrução direta do usuário em 2026-09-05 | Uma página são ~2.500 caracteres; um modelo pequeno recebendo isso de uma vez **se perde** — resume em vez de traduzir, trunca no meio, ou começa a comentar. O parágrafo é a maior unidade que ainda carrega sentido completo e a menor que não quebra frase. **Custo aceito:** muitas requisições curtas em vez de poucas longas; o total de tokens de saída é o mesmo, e a T1 mede se o custo por requisição muda a conta |
| **Parágrafo que atravessa duas páginas** | **Não acontece**, e isso não é sorte | A paginação da READ-10 já quebra em **fronteira de parágrafo**. As duas decisões compõem: uma página é um número inteiro de parágrafos, então traduzir por parágrafo dentro da página nunca corta um parágrafo ao meio. A única exceção é o parágrafo maior que uma página, que a própria paginação já quebra em fronteira de frase — e aí a tradução herda esses pedaços |
| **Checkpoint por parágrafo ou por página?** | **Por página.** A página só é gravada quando todos os parágrafos dela voltaram | Checkpoint por parágrafo exigiria tabela nova ou coluna parcial, para salvar no máximo **uma página** de retrabalho — alguns parágrafos, dezenas de segundos. Esquema novo para economizar isso é caro pelo que entrega |
| **A tradução é o padrão?** | **Não.** O padrão do diálogo é "não traduzir" | Sem tradução, processar é extrair + paginar: segundos, sem LLM. É o caminho que a maioria dos livros vai usar, e ele não pode pagar o preço do que a minoria precisa |
| **Quais idiomas o diálogo oferece** | "Não traduzir (original)", **Português**, **English** | São os dois idiomas que o app já tem em `src/i18n/locales/`. Uma lista maior seria prometer qualidade que ninguém mediu em idioma nenhum |
| **Quem traduz** | O **modelo local, pelo sidecar llama.cpp que o app já embarca** — instrução direta do usuário em 2026-09-05 | Não há chamada de rede em runtime neste produto e não vai haver. O `llama-server` já sobe, já é morto junto com o app (Job Object) e já tem cliente (`providers::llama_server::stream_chat`). Tradutor dedicado (Argos, CTranslate2, API) está **fora**: seria um segundo motor, um segundo download e um segundo ciclo de vida para o que o motor que já existe faz |
| **Com qual modelo** | Um **modelo default designado em código**, e não uma escolha por livro — instrução direta do usuário em 2026-09-05 | O usuário pediu "um modelo default escolhido": traduzir não pode obrigar a escolher modelo toda vez. Hoje **não existe nenhuma constante de modelo padrão no código** — `grep` por `DEFAULT_MODEL` não devolve nada, e o "padrão Phi-3.5" do `ROADMAP.md` é curadoria escrita em prosa, nunca um default executável. Esta feature cria essa constante |
| **Qual modelo vira o default** | **Decidido por medição na T1**, entre os candidatos do catálogo curado — não afirmado aqui | Nomear um agora seria afirmar qualidade de tradução que ninguém mediu, que é exatamente o que a L-003 registra ter custado um dia inteiro. O catálogo tem 6 entradas (`models/catalog.rs`) e a T1 roda a **mesma página real** nos candidatos viáveis, comparando qualidade e tokens/s. O vencedor é escrito no design e vira a constante |
| **E se o modelo default não estiver instalado** | O diálogo oferece baixá-lo, pelo fluxo de download que **já existe** (`download_model`, com progresso) | Degrau 2 da escada: o download curado, a barra de progresso e a validação de URL `.gguf` já estão na árvore e são reusados inteiros. Nada novo é escrito para isso |
| **Como o usuário troca o modelo de tradução** | Pelo painel de edição do livro, que **chama o `set_active_model` que já existe** — instrução direta do usuário em 2026-09-05 | Escolher o modelo ativo já é uma funcionalidade pronta na tela de Runtime (`set_active_model`, `list_installed_models`). O painel de edição reusa os dois em vez de criar um segundo conceito de "modelo default". Isso **remove** um conceito do plano anterior em vez de acrescentar um |
| **Reprocessar uma página é re-extrair?** | **Não. Reprocessar página é só retraduzir** | Re-extrair mudaria a paginação e, com ela, todos os índices de página — a posição salva do usuário e as outras páginas traduzidas iriam junto. Extração ruim se conserta reprocessando o **livro** (READ-13), que já existe e já clampa a posição |
| **Como uma página é marcada para retraduzir** | `UPDATE book_pages SET translated_text = NULL` nas páginas escolhidas, e o laço de tradução as pega | O checkpoint de retomada **já é** "a próxima página sem tradução". Limpar é marcar. Nenhuma coluna de estado por página, nenhuma fila: a estrutura que a retomada exigia serve inteira para o reprocessamento seletivo |
| **Trocar o idioma de um livro já traduzido** | ⛔ **REVOGADA em 2026-09-05, pelo usuário.** ~~Limpa todas as traduções~~ → **nada é apagado**: o idioma de leitura só escolhe qual pasta o leitor lê | Esta linha estava marcada como suposição a vetar, e foi vetada. Ela partia de que só uma tradução existiria por livro, e por isso trocar de idioma tinha de destruir a anterior — o preço era retraduzir horas para voltar atrás. Com uma pasta por idioma, o problema **deixa de existir** em vez de ser mitigado |
| **O layout dos arquivos** | `library/<pasta do livro>/<arquivo>` mais `original/`, `pt/`, `en/`… cada uma com `0001.txt`, `0002.txt`… | Decidido com o usuário em 2026-09-05. Um arquivo por página, e não um por idioma, porque é o que torna "reprocessar a página 2" um `remove_file` em vez de reescrever o livro inteiro — e é o que permite corrigir uma página à mão num editor de texto |
| **O livro passa a morar em pasta própria** | Sim. `library/<pasta>/<arquivo>` no lugar de `library/<arquivo>` | Decidido com o usuário. **Isto revoga o LIB-02 como está escrito** e exige migrar bibliotecas existentes — ver READ-32. Um diretório e um arquivo com o mesmo nome não coexistem no mesmo filesystem, então manter o arquivo na raiz e criar uma pasta homônima ao lado era impossível |
| **Como a pasta do livro é nomeada** | Coluna nova `folder` em `books`, preenchida com o nome do arquivo sem extensão, desambiguada por sufixo como o `unique_destination` já faz | Derivar a pasta do `filename` em tempo de execução parece mais barato até `a.pdf` e `a.epub` quererem a mesma pasta. Guardar o nome elimina a regra de deriva\u00e7ão e o bug que ela esconde |
| **Quantas páginas cada idioma tem** | Contado do disco, não guardado no banco | É um `read_dir` da pasta do idioma. Guardar no banco criaria uma segunda fonte de verdade que o usuário pode contrariar apagando um arquivo pelo explorador — e ele **pode**, porque a pasta é dele |
| **Onde as traduções moram** | Uma pasta por idioma, dentro da pasta do livro | Coluna única no banco só cabe **uma** tradução por página; colunas por idioma exigiriam migração a cada idioma novo. A pasta não impõe teto e é o que o usuário pediu |
| **O índice de página é o mesmo em todos os idiomas?** | **Sim, e isso é propriedade de projeto, não coincidência** | A paginação roda sobre o texto **original** e a tradução é por página. Logo `pt/0047.txt` e `en/0047.txt` são a mesma página, e **a posição de leitura continua sendo do livro**, não do idioma. Se a tradução fosse repaginada, cada idioma teria uma numeração e o histórico precisaria de uma posição por idioma |
| **O que o leitor mostra** | A tradução no idioma corrente, se o arquivo existir; senão, o original | Um idioma parcialmente traduzido fica legível durante o trabalho, em vez de mostrar página em branco |
| **Progresso e reprocessamento são por livro ou por idioma?** | **Por idioma** | Um livro pode estar 100% em português e 30% em inglês ao mesmo tempo. Uma barra só mentiria sobre os dois |
| **`books.status` continua tendo `translating`?** | **Não.** O status do livro descreve só extração e paginação | Traduzir deixou de ser estado do livro quando virou trabalho por idioma. O andamento de cada idioma é derivado do disco. Isso **remove** um estado da máquina em vez de acrescentar |
| **O livro guarda qual modelo o traduziu?** | **Não** | Seria uma coluna por página (por página, porque uma página pode ser retraduzida com outro modelo). O fluxo que o usuário descreveu não precisa dela: ele **lê**, vê que a página ficou ruim, e manda reprocessar. Proveniência entra quando alguém precisar responder "quais páginas estão velhas", que não é a pergunta de hoje |
| **Tradução usa o modelo ativo ou força o default?** | **Usa o modelo ativo.** O `DEFAULT_TRANSLATION_MODEL` encolheu para um papel só: é o modelo **sugerido para download** quando nenhum está instalado | `set_active_model` **reinicia o sidecar** (`runtime_commands.rs:509`). Manter um segundo modelo só para traduzir custaria dois reinícios por livro e um segundo slot de modelo no esquema. Como o chat está sendo revogado, o app passa a ter **um** modelo, e o default designado é o valor inicial dele — não um segundo |
| **Quais formatos ganham leitor** | **PDF e EPUB.** MOBI/AZW/AZW3 são recusados **na lista**, não só no clique | PDF já tem extração provada contra um PDF real de 500 páginas (`rag::parsing::extract_pdf` via pdfium, AD-032/L-003). EPUB é zip de XHTML e o crate `zip` **já é dependência** (`Cargo.toml:38`) — degrau 5 da escada, sem lib nova. MOBI/AZW não têm parser vetado. O Crítico e o Pragmático apontaram a mesma falha na versão anterior desta escolha: recusar só no clique deixa quem nunca clica sem saber. Por isso a recusa é **estado visível na linha** |
| **Ordem do texto no EPUB** | Pelo **spine do `.opf`**, resolvido a partir de `META-INF/container.xml` | Ordem alfabética das entradas do zip **não é** a ordem de leitura, e o Crítico está certo que "zip de XHTML" esconde isso. Ler o spine é o único caminho correto e custa uma leitura de XML a mais |
| **O que é uma "posição de leitura"** — *o bloqueador da `reading-history`* | **Índice de página** (`INTEGER`, base 0) na paginação persistida do livro | Esta era a pergunta que travava a `reading-history`, e ela só podia ser respondida por esta feature. Offset de caractere e índice de parágrafo foram descartados: a paginação é determinística e persistida, então a página **é** a unidade que o usuário vê e a que o app grava. Reprocessar regenera as páginas, e aí a posição é **clampada** ao novo total, nunca deslocada em silêncio |
| **Onde o texto remontado mora** | ⛔ **REVOGADA em 2026-09-05, pelo usuário.** ~~Tabela `book_pages` no SQLite~~ → **arquivos no disco**, um por página, numa pasta por idioma dentro da pasta do livro | O usuário pediu explicitamente pastas separadas por idioma, dentro da pasta do livro. E a escolha se paga: o mesmo `library/` que a LIB-11 já abre no explorador passa a conter o texto **legível e editável à mão**, o que é a forma mais direta de consertar uma tradução ruim. Reprocessar uma página vira `remove_file` — inclusive se o usuário apagar o arquivo pelo explorador. **O banco encolhe:** a migração 10 deixa de criar tabela e vira só colunas em `books` |
| **Onde a sessão de leitura mora** | Colunas em `books` (`last_page`, `last_opened_at`), **não** tabela nova e **não** reuso de `chats` | Um livro tem no máximo uma leitura corrente; uma tabela `reading_sessions` com no máximo uma linha por livro seria uma tabela para nada. Reusar `chats`/`messages` foi levantado pelo Cético e descartado pelo Crítico com o argumento certo: a regra "turnos precisam alternar" (`AGENTS.md`) não tem significado nenhum para "página 47", e forçar o encaixe misturaria dois domínios numa migração só |
| **A lista de chats some ou convive com o histórico?** | **Some.** A lateral passa a listar leituras | O council votou 3×0 por **conviver**. Rejeitado **porque o repositório já respondeu**: `reading-history/spec.md` HIST-01 diz, em EARS, "SHALL NOT listar conversas de chat", e a AD-052 item 4 já registrou a revogação com gatilho escrito. As três vozes não tinham esses dois documentos. A regra do `.claude/rules/spec-driven-changes.md` é explícita: não se pergunta o que o repositório já responde |
| **O `ChatPanel` é apagado agora?** | **Não.** Fica órfão de rota, compilando. ⚠️ **Corrigido pela execução (2026-09-06):** o `ChatPanel` de fato ficou órfão e compilando, **mas o `DocumentsPanel.tsx` desta comparação teve de ser apagado** — ele chamava `setActiveView("chat")` e o `tsc` o derrubou junto com o `ChatList.tsx`. A previsão errou por dois arquivos, e a armadilha que a T11 documentava é exatamente essa | Mesmo padrão da AD-052 e da T7 da `book-library`. A objeção do Pragmático e do Crítico (código morto que ninguém limpa) é legítima **e já tem resposta escrita**: o gatilho da remoção é justamente esta feature renderizar um livro ponta a ponta. Ela deixa de ser "depois" e vira a próxima — READ-19 |
| **Imagens no texto** | Fora, **declarado para o usuário vetar** | Ver Out of Scope. É a única divergência entre o pedido literal e o `ROADMAP.md`, e as três vozes do council convergiram em não decidi-la em silêncio |

Open questions: none bloqueando o planejamento. Uma permanece aberta e bloqueia uma única task (a T6), com regra de parada escrita.

**Open question que sobrevive a este planejamento, e ela é a única:** o Phi-3.5 Mini 3.8B Q4_K_M produz tradução **utilizável** de prosa longa, e a que velocidade nesta máquina? Isso não foi medido nunca neste projeto. A T1 mede, e carrega a regra de parada escrita: **se a saída for inutilizável numa página real, a T6 não é construída e o usuário é consultado** — construir um tradutor que produz lixo é trabalho jogado fora, e é o único ponto desta feature em que parar e perguntar vale mais do que entregar.

---

## User Stories

### P1: Ver o estado do livro e mandar processar ⭐ MVP

**User Story**: Como leitor, quero ver na lista o nome do arquivo que importei e um botão para processá-lo, para saber o que já está pronto para ler e o que ainda não está.

**Acceptance Criteria**:

1. WHEN a Biblioteca é renderizada THEN cada livro SHALL mostrar o nome do arquivo e o seu estado de processamento
2. WHERE o formato do livro é `pdf` ou `epub` AND ele ainda não foi processado WHEN a linha é renderizada THEN o sistema SHALL oferecer a ação de processar
3. WHERE o formato do livro é `mobi`, `azw` ou `azw3` WHEN a linha é renderizada THEN o sistema SHALL indicar que a leitura ainda não é suportada para esse formato, e SHALL NOT oferecer a ação de processar
4. WHEN um livro já processado é renderizado THEN o sistema SHALL oferecer a ação de ler e SHALL mostrar o total de páginas
5. WHILE um livro está sendo processado THEN o sistema SHALL mostrar o progresso na linha dele e SHALL oferecer o cancelamento

**Independent Test**: importar um PDF, um EPUB e um MOBI; as duas primeiras linhas oferecem "Processar", a terceira diz que a leitura não é suportada e não oferece o botão.

---

### P1: Escolher o idioma antes de processar ⭐ MVP

**User Story**: Como leitor, quero escolher em que idioma o livro vai ficar antes de o processamento começar, para ler em português um livro que veio em inglês.

**Acceptance Criteria**:

1. WHEN o usuário aciona o processamento THEN o sistema SHALL perguntar o idioma de leitura antes de iniciar, oferecendo "não traduzir (original)", português e inglês
2. WHEN o diálogo é aberto THEN a opção pré-selecionada SHALL ser "não traduzir (original)"
3. IF o usuário escolhe um idioma de tradução WHEN o diálogo é confirmado THEN o sistema SHALL mostrar a estimativa de tempo medida antes de iniciar
4. WHEN o processamento termina THEN o sistema SHALL registrar no livro qual idioma foi escolhido

**Independent Test**: acionar "Processar" num EPUB em inglês, escolher português, e confirmar que a estimativa aparece antes do início e que o idioma registrado é `pt` depois do fim.

---

### P1: Extrair e remontar em páginas ⭐ MVP

**User Story**: Como leitor, quero que o app transforme o arquivo num texto paginado, para que eu possa ler como um livro em vez de rolar um bloco único.

**Acceptance Criteria**:

1. WHEN um PDF é processado THEN o sistema SHALL extrair o texto pelo mesmo caminho que a base já usa para documentos
2. WHEN um EPUB é processado THEN o sistema SHALL extrair o texto dos documentos do spine, **na ordem do spine**, resolvido a partir de `META-INF/container.xml`
3. WHEN o texto é extraído THEN o sistema SHALL dividi-lo em páginas de tamanho determinístico, quebrando em fronteira de parágrafo, e SHALL persistir cada página com o seu índice
4. WHEN o processamento termina com sucesso THEN o sistema SHALL registrar o total de páginas no livro
5. IF o arquivo não produz texto extraível WHEN ele é processado THEN o sistema SHALL registrar o erro com a mensagem, SHALL deixar o livro em estado de erro, e SHALL NOT deixar páginas parciais gravadas
6. WHEN o mesmo texto é paginado duas vezes THEN o sistema SHALL produzir exatamente as mesmas páginas

**Independent Test**: processar um EPUB cujo spine está fora da ordem alfabética dos arquivos do zip e confirmar que a página 0 é o primeiro capítulo do spine, não o primeiro arquivo do zip.

---

### P1: Traduzir quando o usuário pedir ⭐ MVP

**User Story**: Como leitor, quero que o livro seja traduzido para o idioma que escolhi, para ler no meu idioma um livro que não veio nele.

**Acceptance Criteria**:

1. IF o idioma escolhido é "não traduzir" WHEN o processamento roda THEN o sistema SHALL NOT chamar o modelo, e o processamento SHALL terminar na paginação
2. IF um idioma de tradução foi escolhido WHEN o processamento roda THEN o sistema SHALL percorrer as páginas **na ordem**, gravando cada uma assim que ela fica pronta
2a. WHEN uma página é traduzida THEN o sistema SHALL enviar **um parágrafo por requisição**, na ordem, e SHALL NOT enviar a página inteira num único prompt
2b. WHEN os parágrafos de uma página voltam THEN o sistema SHALL remontá-los preservando as quebras de parágrafo do original
3. WHILE a tradução está em curso THEN o sistema SHALL emitir o progresso em páginas traduzidas sobre o total
4. WHEN o usuário cancela a tradução THEN o sistema SHALL parar após a página corrente e SHALL preservar as páginas já traduzidas
5. IF o app é fechado durante a tradução WHEN o livro é processado de novo com o mesmo idioma THEN o sistema SHALL retomar a partir da primeira página ainda não traduzida
6. WHEN uma página tem tradução gravada THEN o leitor SHALL exibir a tradução, e o texto original SHALL continuar gravado
7. IF a tradução de uma página falha WHEN a tradução roda THEN o sistema SHALL parar, registrar o erro, e SHALL preservar as páginas já traduzidas
8. WHEN a tradução roda THEN o sistema SHALL usar o modelo local servido pelo sidecar llama.cpp, e SHALL NOT chamar nenhum serviço de rede
9. IF nenhum modelo está ativo WHEN o usuário pede tradução THEN o sistema SHALL nomear o modelo default de tradução e SHALL oferecer o download dele, sem apagar as páginas já existentes
10. IF a tradução de um parágrafo volta vazia WHEN a tradução roda THEN o sistema SHALL tratar isso como falha da página, e SHALL NOT gravar a página com o parágrafo faltando

**Independent Test**: processar um livro de 5 páginas para português, cancelar na terceira, reabrir e reprocessar no mesmo idioma; a tradução recomeça na página 3 e as duas primeiras não são refeitas.

---

### P1: Editar o livro e reprocessar o que ficou ruim ⭐ MVP

**User Story**: Como leitor, quero abrir uma configuração do livro para trocar o idioma, trocar o modelo e mandar reprocessar só as páginas que ficaram mal traduzidas, para consertar o que saiu ruim sem refazer o livro inteiro.

**Acceptance Criteria**:

1. WHEN o usuário aciona a edição de um livro processado THEN o sistema SHALL abrir um painel com o idioma de leitura, o modelo em uso e a lista de páginas
2. WHEN o painel é aberto THEN o sistema SHALL listar os modelos instalados e SHALL permitir escolher qual será usado, aplicando a escolha pelo mesmo caminho que a tela de Runtime já usa
3. IF nenhum modelo está instalado WHEN o painel é aberto THEN o sistema SHALL nomear o modelo default de tradução e SHALL oferecer o download dele
4. WHEN o usuário troca o modelo THEN o sistema SHALL avisar que o runtime será reiniciado antes de aplicar
5. WHEN o usuário seleciona uma ou mais páginas e manda reprocessar THEN o sistema SHALL retraduzir **apenas** aquelas páginas, e SHALL NOT tocar nas demais
6. WHEN páginas são reprocessadas THEN o sistema SHALL NOT re-extrair nem repaginar o livro, e os índices de página SHALL permanecer os mesmos
7. WHEN o painel é aberto THEN o sistema SHALL listar os idiomas que o livro já tem, cada um com quantas páginas estão traduzidas, e SHALL permitir acrescentar um idioma novo ou remover um existente
8. WHEN o usuário manda reprocessar o livro inteiro pelo painel THEN o sistema SHALL re-extrair, repaginar e retraduzir, clampando a posição salva
9. WHILE um reprocessamento seletivo está em curso THEN o sistema SHALL mostrar o progresso e SHALL permitir cancelar, preservando as páginas já refeitas

**Independent Test**: num livro de 10 páginas traduzido, marcar só a página 4, reprocessar, e confirmar no banco que apenas a linha da página 4 mudou de `translated_text` e que `page_count` continua 10.

---

### P1: Vários idiomas no mesmo livro, em pastas separadas ⭐ MVP

**User Story**: Como leitor, quero que o mesmo livro guarde traduções em mais de um idioma ao mesmo tempo, cada uma na sua pasta dentro da pasta do livro, para alternar sem perder trabalho e para conseguir mexer nos arquivos por fora.

**Acceptance Criteria**:

1. WHEN um livro é processado THEN o sistema SHALL gravar o texto extraído em arquivos, **um por página**, na pasta `original/` dentro da pasta do livro
2. WHEN um livro é traduzido para um idioma THEN o sistema SHALL gravar cada página traduzida como um arquivo próprio numa pasta com o nome daquele idioma, e SHALL preservar as pastas dos outros idiomas
3. WHEN o usuário troca o idioma de leitura THEN o sistema SHALL passar a ler daquela pasta, e SHALL NOT apagar nenhum arquivo
4. WHERE o arquivo da página não existe na pasta do idioma corrente THEN o leitor SHALL exibir a página de `original/`
5. WHEN o usuário remove um idioma THEN o sistema SHALL apagar apenas a pasta daquele idioma, e SHALL informar quantas páginas serão perdidas antes de aplicar
6. WHEN um livro é removido da Biblioteca THEN o sistema SHALL remover a pasta do livro inteira, com todos os idiomas
7. WHEN o progresso de tradução é exibido THEN ele SHALL ser por idioma, contado dos arquivos existentes, e SHALL NOT somar idiomas diferentes numa barra só
8. WHEN o usuário alterna entre dois idiomas THEN a página corrente SHALL ser a mesma nos dois
9. IF o usuário apaga o arquivo de uma página pelo explorador WHEN aquela página é reprocessada THEN o sistema SHALL tratá-la como não traduzida e refazê-la
10. WHERE o livro tem só o `original/` THEN o leitor SHALL NOT mostrar o seletor de idioma; com mais de um idioma, o seletor SHALL aparecer no cabeçalho (READ-33, AD-070)

**Independent Test**: traduzir um livro de 10 páginas para português e depois para inglês; a pasta do livro tem `original/`, `pt/` e `en/` com 10 arquivos cada, e alternar o idioma na página 6 mantém a página 6.

---

### P1: O livro ganha pasta própria ⭐ MVP

**User Story**: Como leitor, quero que cada livro tenha uma pasta só dele dentro da biblioteca, para achar o arquivo e os textos processados juntos quando eu abrir a pasta no explorador.

**Acceptance Criteria**:

1. WHEN um livro é importado THEN o sistema SHALL criar uma pasta para ele dentro de `library/` e SHALL colocar o arquivo dentro dela
2. IF duas pastas disputariam o mesmo nome WHEN um livro é importado THEN o sistema SHALL desambiguar com sufixo numérico, do mesmo jeito que já faz com nome de arquivo
3. WHEN o app abre uma biblioteca criada antes desta mudança THEN o sistema SHALL mover cada livro solto para a pasta dele e SHALL registrar a pasta no banco
4. IF a migração de layout já rodou WHEN o app abre de novo THEN ela SHALL ser um no-op, sem mover nada
5. IF um arquivo não pode ser movido WHEN a migração roda THEN o sistema SHALL registrar o erro e SHALL deixar aquele livro como estava, sem perder a linha nem o arquivo

**Independent Test**: com dois livros importados no layout antigo, abrir o app e confirmar que cada um está dentro da própria pasta e que a lista continua mostrando os dois; abrir de novo e confirmar que nada se moveu.

---

### P1: Ler o livro ⭐ MVP

**User Story**: Como leitor, quero ler o livro na tela uma página por vez, para não perder o lugar num rolo de texto.

**Acceptance Criteria**:

1. WHEN a leitura é aberta THEN o sistema SHALL exibir uma página por vez, com o número da página corrente e o total
2. WHEN o usuário avança ou volta THEN o sistema SHALL exibir a página adjacente
3. WHEN o usuário usa as setas do teclado THEN o sistema SHALL avançar e voltar da mesma forma que os botões
4. WHERE a página corrente é a primeira THEN a ação de voltar SHALL estar indisponível; WHERE é a última, a de avançar SHALL estar indisponível

**Independent Test**: abrir um livro de 3 páginas, ir até a última pela seta do teclado e confirmar que "avançar" fica indisponível.

---

### P1: A lateral vira histórico de leitura ⭐ MVP

**User Story**: Como leitor, quero que a lateral do app mostre minhas leituras em vez de conversas, para voltar rápido ao que eu estava lendo. *(É a `reading-history`, HIST-01…HIST-03)*

**Acceptance Criteria**:

1. WHEN a lateral é renderizada THEN o sistema SHALL listar as leituras, e SHALL NOT listar conversas de chat
2. WHEN há mais de uma leitura THEN o sistema SHALL ordená-las da mais recentemente aberta para a mais antiga
3. IF nenhum livro foi aberto ainda WHEN a lateral é renderizada THEN o sistema SHALL mostrar um estado vazio que aponta para a Biblioteca
4. WHEN o usuário seleciona uma leitura na lateral THEN o sistema SHALL abrir aquele livro na posição salva
5. WHEN uma leitura é listada THEN o sistema SHALL mostrar o nome do livro e a posição (página corrente sobre o total)

**Independent Test**: abrir dois livros em ordem, e confirmar que o segundo aparece no topo da lateral e que a lista de chats não existe mais na tela.

---

### P1: Retomar de onde parei ⭐ MVP

**User Story**: Como leitor, quero reabrir um livro no ponto onde parei, para não procurar a página toda vez. *(É a `reading-history`, HIST-04…HIST-08)*

**Acceptance Criteria**:

1. WHEN um livro é aberto para leitura THEN o sistema SHALL registrar o instante da última abertura
2. WHILE a leitura avança THEN o sistema SHALL persistir a página corrente
3. WHEN o usuário reabre um livro que já tem posição salva THEN o sistema SHALL abrir naquela página
4. IF o livro nunca foi aberto WHEN o usuário o abre THEN o sistema SHALL começar na primeira página
5. WHEN um livro é removido da Biblioteca THEN o sistema SHALL remover também as páginas dele e a entrada no histórico
6. IF o livro foi reprocessado e passou a ter menos páginas WHEN ele é reaberto THEN o sistema SHALL abrir na última página existente, e SHALL NOT apontar para uma página que não existe

**Independent Test**: ler até a página 5, fechar o app, reabrir e selecionar o livro na lateral; ele abre na página 5.

---

## Edge Cases

- WHEN um livro é processado duas vezes THEN as páginas antigas SHALL ser substituídas pelas novas, e a posição salva SHALL ser clampada ao novo total
- WHEN o arquivo do livro sumiu do disco THEN o processamento SHALL falhar com essa mensagem, e a linha do histórico SHALL indicar isso em vez de abrir vazia
- WHEN um PDF só com imagens é processado THEN o erro SHALL ser o mesmo `NoTextFound` que a base já produz hoje ("PDFs digitalizados precisam de OCR"), não um livro de zero páginas
- WHEN um EPUB não tem `META-INF/container.xml`, ou o `.opf` não tem spine THEN o processamento SHALL falhar com essa mensagem, e SHALL NOT cair para a ordem das entradas do zip
- WHEN o runtime não está no ar AND o usuário pede tradução THEN o sistema SHALL recusar antes de apagar as páginas existentes, com a mensagem de que o modelo não está disponível
- WHEN o download do modelo default é cancelado pelo usuário THEN o livro SHALL continuar no estado em que estava, e SHALL NOT ficar em `error`
- WHEN o app é fechado no meio de uma página THEN os parágrafos já traduzidos daquela página SHALL ser perdidos e a página SHALL ser refeita inteira — perda deliberada de no máximo uma página, aceita para não criar checkpoint por parágrafo
- WHEN uma página tem um único parágrafo maior que ela THEN a tradução SHALL usar os pedaços que a paginação já quebrou em fronteira de frase, e SHALL NOT mandar o parágrafo inteiro
- WHEN o cancelamento chega no meio de uma página THEN o sistema SHALL parar ao fim do parágrafo corrente, e a página incompleta SHALL NOT ser gravada
- WHEN o usuário troca o modelo ativo entre duas execuções do mesmo livro THEN as páginas já traduzidas SHALL ser mantidas — misturar dois tradutores num livro é aceito de propósito, porque refazer o que já saiu custaria horas de CPU para uma consistência que ninguém pediu
- WHEN o livro é reprocessado inteiro (re-extração) THEN **todas** as pastas de idioma SHALL ser apagadas — a repaginação muda os índices, e um `pt/0047.txt` preso a um índice que mudou apontaria para o trecho errado. O painel SHALL dizer isso, com a contagem por idioma, antes de aplicar
- WHEN o usuário manda reprocessar uma página com o idioma de leitura no original THEN o sistema SHALL pedir para qual idioma, porque não há tradução corrente a refazer
- WHEN o usuário remove o idioma que está sendo lido THEN o idioma de leitura SHALL voltar ao original
- WHEN a pasta do livro é apagada pelo explorador THEN a lista SHALL continuar mostrando a linha até que o usuário a remova pelo app, e abrir a leitura SHALL informar que os arquivos sumiram — mesma regra que a `book-library` já adotou para o arquivo
- WHEN o usuário manda reprocessar uma página que ainda não foi traduzida THEN a operação SHALL ser um no-op bem-sucedido, não um erro
- WHEN o livro é reprocessado inteiro AND o usuário tinha páginas selecionadas no painel THEN a seleção SHALL ser descartada, porque os índices podem ter mudado
- WHEN a última página de um livro é a corrente AND o livro é reprocessado para mais páginas THEN a posição SHALL ser mantida, não movida para o novo fim
- WHEN o usuário remove um livro que está sendo processado THEN o processamento SHALL parar e SHALL NOT recriar as linhas removidas

---

## Requirement Traceability

| Requirement ID | Story | Phase | Status |
| --- | --- | --- | --- |
| READ-01 | P1: Nome do arquivo + estado na lista | **T10** escreveu a tela da Biblioteca: `BookRow.tsx` (+149 linhas), `LibraryPanel.tsx` e `ProcessDialog.tsx` (novo): a linha mostra nome, `FORMATO · tamanho` e o rótulo de estado (`STATUS_LABEL_KEY`, um por variante de `BookStatus`) | **não verificado — só compila.** `npm run build` **exit 0** (1.861 módulos, 2,86 s, `tsc` limpo). **Nada foi visto na tela, nenhum `invoke` foi disparado** e não há suíte de frontend. Comportamento é T13 |
| READ-02 | P1: Ação de processar em PDF e EPUB | T5 escreveu `process_into_pages` e o comando `process_book` | **metade verificada (backend)** — `only_pdf_and_epub_can_be_processed` e `a_processed_book_fills_original_and_records_page_count` em **238/0/16**, contra banco em memória + pasta temporária, com EPUB sintético. pending — **a ação na tela é T10/T13**, e o comando Tauri nunca rodou (não há runner de integração) |
| READ-03 | P1: MOBI/AZW/AZW3 marcados como não suportados na lista | T5: `book_paths` recusa antes de escrever qualquer status | **metade verificada** — `only_pdf_and_epub_can_be_processed`: `.mobi`, `.azw` e `.azw3` devolvem `formato não suportado: .<ext>` (mensagem reusada de `ParseError::UnsupportedFormat`, sem tipo de erro novo) e o livro **continua em `imported`, com `page_count = 0`**. pending — **o rótulo na lista é a T13** |
| READ-04 | P1: Livro pronto oferece ler e mostra o total de páginas | **T10** escreveu a tela da Biblioteca: `BookRow.tsx` (+149 linhas), `LibraryPanel.tsx` e `ProcessDialog.tsx` (novo): `isReady = status === "ready" && page_count > 0` troca o botão Processar por **Ler** + lápis de editar, e o rótulo passa a ser `library.pages` | **não verificado — só compila.** `npm run build` **exit 0** (1.861 módulos, 2,86 s, `tsc` limpo). **Nada foi visto na tela, nenhum `invoke` foi disparado** e não há suíte de frontend. Comportamento é T13 |
| READ-05 | P1: Progresso e cancelamento na linha | **T10** escreveu a tela da Biblioteca: `BookRow.tsx` (+149 linhas), `LibraryPanel.tsx` e `ProcessDialog.tsx` (novo): a barra vem do evento `book-status` guardado em `libraryStore.progress[id]` (não há status `translating`), com `done/total` absolutos para a retomada não voltar a zero; enquanto `isBusy` o único botão é **Cancelar** | **não verificado — só compila.** `npm run build` **exit 0** (1.861 módulos, 2,86 s, `tsc` limpo). **Nada foi visto na tela, nenhum `invoke` foi disparado** e não há suíte de frontend. Comportamento é T13 |
| READ-06 | P1: Diálogo de idioma, com "não traduzir" pré-selecionado | **T10** escreveu a tela da Biblioteca: `BookRow.tsx` (+149 linhas), `LibraryPanel.tsx` e `ProcessDialog.tsx` (novo): `ProcessDialog` usa `<dialog>.showModal()` (Escape e foco preso de graça, sem biblioteca), `useState(ORIGINAL)` deixa "não traduzir" marcado ao abrir · **critério 4 (registrar o idioma escolhido no livro) NÃO era implementado até 2026-09-07**: `process_book` recebia o idioma, traduzia para ele e nunca escrevia `reading_language`, então o leitor abria em `original/` depois de ~63 min de tradução. **T13, defeito 3:** `process_into_pages` ganhou `language: Option<&str>` e chama `set_book_reading_language` ao fim da paginação, antes do evento `ready`; `None` grava NULL porque `wipe_languages` já apagou todas as pastas | **critério 4: verificado por teste automatizado** — `reader_commands::tests::processing_records_the_chosen_language_on_the_book`, que falha com o conserto revertido (`left: None / right: Some("pt")`). Suíte **266 / 0 / 17**. **Critérios 1, 2 e 3 continuam não verificados — só compilam** (`npm run build` exit 0, 1.861 módulos, `tsc` limpo): **nada foi visto na tela e nenhum `invoke` foi disparado**, e não há suíte de frontend. O diálogo em si é T13 |
| READ-07 | P1: Estimativa de tempo medida, antes de iniciar a tradução | T1 mediu o número (~63 min / 300 páginas) | pending — a tela que o mostra é a T13 · **T10:** o `ProcessDialog` mostra a estimativa **antes de começar** e só quando um idioma é escolhido: `SECONDS_PER_TRANSLATED_PAGE = 12.5`, o número medido na T1, vezes `page_count`, com a nota de que é extrapolação de uma página. **Não verificado — só compila** (`npm run build` exit 0); ninguém viu esse texto na tela, e **a estimativa nunca foi comparada com um livro inteiro de verdade** (T13, item 7) |
| READ-08 | P1: Extração de PDF pelo caminho já existente | T5 tornou `rag::parsing::extract_pdf` `pub(crate)` e a chama para `.pdf` — uma linha, sem duplicar `rejoin_hyphenated_words`; **a T13 corrigiu `rag::pdfium::extract_text`, que quebrava no SEGUNDO PDF de cada execução** (AD-057) | ⚠️ **A rota foi exercitada pela primeira vez em 2026-09-06, e ela estava quebrada.** O usuário abriu o app, mandou processar um PDF e recebeu *"não foi possível ler o arquivo: não foi possível carregar o pdfium: PdfiumLibraryBindingsAlreadyInitialized"*. Corrigido e provado **contra a `pdfium.dll` vendored e um PDF de verdade**: `extracting_two_pdfs_in_the_same_process_reuses_the_bindings` (`#[ignore]`, caminhos por env var) passa com o conserto e **falha com a mensagem exata do defeito** quando a correção é revertida — o sensor foi rodado, não presumido. **Continua pendente:** um PDF de livro real, com muitas páginas, extraído pelo app inteiro; o que passou pelo teste foi um PDF de uma página construído para o teste. Evidência anterior, ainda verdadeira: **nenhum PDF havia sido extraído por esta rota**: a pdfium é resolvida por `AppHandle` (`rag::pdfium::ensure_for`), fora do alcance de um teste unitário. O que está provado é que a chamada existe e compila; texto real é a T13 |
| READ-09 | P1: Extração de EPUB na ordem do spine | T3 escreveu `reader::epub::extract_epub_text` | **verificado contra fixture sintético** — `chapters_come_out_in_spine_order_not_zip_order` (zip `c, a, b`, spine `b, a, c`, saída `b, a, c`), + 4 testes de tags/entidades/erro, em 204/0/15. **Nenhum EPUB real passou por aqui**; isso é a T13 |
| READ-10 | P1: Paginação determinística em fronteira de parágrafo | T4 escreveu `reader::pagination::{paginate, split_paragraphs}` | **verificado como função pura** — 6 testes em 210/0/15: `pages_break_on_paragraph_boundaries` (todo fim de página é começo de parágrafo, e nenhuma passa de 2.500 caracteres), `a_paragraph_larger_than_a_page_falls_back_to_sentence_boundaries`, `a_sentence_larger_than_a_page_is_cut_at_the_budget_and_loses_nothing` (`pages.concat() == texto`), `an_empty_text_produces_no_pages`. O `the_same_text_paginates_the_same_way_twice` **não prova determinismo entre execuções** e diz isso dentro do teste. **Nenhum texto de livro real foi paginado** e ninguém chama `paginate` ainda — ponta a ponta é T5/T13 |
| READ-11 | P1: Falha de extração não deixa páginas parciais | T3 cobriu a metade do erro de EPUB; **T5 fecha a outra**: a limpeza só roda depois que há texto novo | **verificado** — `extraction_failure_leaves_original_empty_and_page_count_zero` (238/0/16): um `.epub` que não é zip falha **depois** de o status já ter ido para `extracting`, e o resultado é `status = 'error'` com a mensagem gravada, `page_count = 0` e **zero arquivos `.txt`** na pasta do livro (varredura recursiva). Vale também para o cancelamento: `a_cancelled_processing_writes_no_page_and_goes_back_to_imported` |
| READ-12 | P1: Tradução página a página, gravada à medida que sai | T6 escreveu `reader::translate::translate_book` e ligou o laço ao fim de `process_book` | **metade verificada (backend)** — `a_translated_page_leaves_the_original_file_untouched` em **249/0/16**: 2 páginas traduzidas por um **duble** de tradutor viram `pt/0001.txt` e `pt/0002.txt`, e os bytes de `original/` são conferidos byte a byte antes e depois. A gravação é por página e acontece só quando todos os parágrafos voltaram. pending — **a barra na tela é T10/T13**, o comando `process_book` nunca rodou (não há runner de integração Tauri) e **nenhum modelo foi chamado por teste nenhum** · **T8:** `BookPage.language` chega ao `readerStore` como `pageLanguage`, distinto do idioma pedido, e o listener de `book-status` recarrega a página quando a tradução dela sai — **nada disso rodou**: `npm run build` exit 0 só prova que compila, e nenhum componente importa o store ainda (T9) · **T9:** o `ReaderPanel` rotula a queda para o original numa faixa âmbar quando `pageLanguage !== language`. **Só compila** (`npm run build` exit 0, `tsc` o inclui) — o componente não tem rota (T11) e ninguém o montou; **a faixa nunca apareceu na tela** (T13) |
| READ-13 | P1: Reprocessar substitui as páginas e clampa a posição | T2 criou `last_page`; T5 escreveu `wipe_languages` + o `UPDATE ... last_page = MIN(last_page, page_count - 1)` | **verificado** — 3 testes em 238/0/16: `reprocessing_wipes_every_language_folder_before_regenerating` (`pt/` e `en/` gravadas à mão somem no reprocessamento, e `original/` volta preenchida), `reprocessing_into_fewer_pages_clamps_the_saved_position` (`last_page = 40` num livro que passa a ter 1 página vira `0`, nunca 40) e `reprocessing_into_more_pages_keeps_the_saved_position` (a posição não pula para o novo fim). `MIN` do SQLite é NULL-safe: livro nunca aberto continua com `last_page` NULL, e isso está afirmado no primeiro teste. **`last_page` é índice base 0** |
| READ-14 | P1: Cancelar e retomar a tradução sem refazer o que já saiu | T16 deu o mecanismo (`next_missing`); T6 escreveu o laço que o dirige e confere o cancelamento entre parágrafos | **verificado como laço, com duble** — 4 testes em 249/0/16: `next_untranslated_returns_the_lowest_page_without_a_translation` (buraco no meio, não o fim), `next_untranslated_returns_none_when_every_page_is_done`, `pages_translated_before_a_cancel_are_not_lost` (cancela depois de 2 de 4 páginas; a rodada seguinte pede **só** "Page 3" e "Page 4") e `a_cancel_mid_page_leaves_the_page_untranslated` (cancelado no 1º de 4 parágrafos: 1 requisição, arquivo nenhum, página volta a pendente). pending — o botão de cancelar na tela é T10/T13 |
| READ-15 | P1: Leitor exibe uma página por vez, com teclado | **T9** escreveu `src/components/Reader/ReaderPanel.tsx`: uma página por vez (`whitespace-pre-wrap`), anterior/próxima, `x de y` (base 0 → base 1 só aqui), `ArrowLeft`/`ArrowRight` em `window` com cleanup no `useEffect`, botões `disabled` nas pontas | **não verificado — só compila.** `npm run build` exit 0 e `npx tsc --listFilesOnly` confirma que o arquivo é compilado. **Nenhuma tecla foi pressionada, nenhuma página virou, nada foi visto na tela**, e o componente **não está roteado** (`ActiveView` não tem `"reader"` — é T11). Comportamento é T13 · **T11 fechou a rota:** `ActiveView` passou a ser `"reader" | "settings" | "runtime" | "library"`, o padrão do `uiStore` virou `"reader"` e o `App.tsx` monta o `ReaderPanel` no lugar do `ChatPanel`. `npm run build` **exit 0** (1.861 módulos). **Continua sem nenhuma tecla pressionada e sem nenhum pixel visto** — o que mudou é que agora existe caminho até a tela; que ela renderize é T13 |
| READ-16 | P1: Abrir da Biblioteca vai para a posição salva | T2 criou as colunas; **T7 escreveu `open_position`** (grava `last_opened_at`, devolve `last_page` clampado) e `page_text` | **metade verificada (backend)** — 3 testes em **257/0/16**: `reopening_a_book_returns_the_saved_page` (salva 4, reabre em 4, base 0 dos dois lados), `a_book_never_opened_starts_at_the_first_page` (`last_page` NULL → 0, **e a coluna continua NULL**) e `a_position_beyond_the_page_count_is_clamped_on_open` (posição 40 num livro de N páginas volta N-1). `page_text` recusa página ≥ `page_count` em vez de procurar arquivo. pending — **o comando `open_book` nunca rodou** (não há runner de integração Tauri) e **abrir pela Biblioteca é T9/T10** · **T8:** `readerApi.openBook` + `readerStore.openBook` escritos (a posição vem do backend, nunca é adivinhada da linha). `npm run build` exit 0; **nenhuma chamada real** · **T9:** o painel consome `page`/`pageCount` do store, mas **não chama `openBook`** — quem abre é a lista lateral (T11) ou a linha da Biblioteca (T10). **Nenhuma abertura real aconteceu** |
| READ-17 | P1: Posição persistida enquanto a leitura avança | T2 criou `last_page` e `last_opened_at`, ambas NULL até a primeira abertura; **T7 escreveu `save_position`**, com o clamp no próprio SQL (`MIN(?1, MAX(page_count - 1, 0))`) porque o número vem do frontend | **metade verificada (backend)** — `saving_a_position_persists_it` em **257/0/16**: gravar 2 deixa 2 na linha, e gravar 999 deixa `page_count - 1`. pending — **quem chama enquanto a leitura avança é o store da T8/T9, com debounce**, e nada disso existe; o comando `save_reading_position` nunca rodou · **T8:** o debounce existe — `readerStore.goToPage` agenda a gravação em 800 ms e `closeBook` faz *flush* do timer pendente. **O debounce nunca disparou**: não há suíte de frontend e nenhuma tela chama o store (T9) · **T9:** cada clique/tecla chama `goToPage`, que é quem agenda o debounce da T8. **O debounce continua sem nunca ter disparado**: o componente não está montado em lugar nenhum (rota é T11) e não há suíte de frontend |
| READ-18 | P1: Remover o livro remove páginas e histórico | T2 criou `folder`; T16 escreveu `reader::storage::remove_book_dir`; T17 fez `remove_book` chamá-la | **verificado no disco, com páginas de verdade** — `removing_a_book_removes_its_whole_folder` (238/0/16): um livro processado (6 páginas em `original/`) mais uma pasta `pt/` gravada some inteiro — pasta, páginas e arquivo importado — e o livro vizinho, também processado, fica de pé com as suas. A linha de `books` sai no mesmo `remove_book` (o `DELETE` já existia), e **o histórico são as colunas `last_page`/`last_opened_at` da própria linha**, então sai junto — não há tabela separada para cascatear. pending — **o botão da tela é da `book-library` e não foi reexercitado aqui**; o comando `delete_book` continua sem nunca ter rodado |
| READ-19 | — Gatilho da AD-052 disparado e registrado | **T11/T12 registraram, e o gatilho NÃO disparou.** A AD-052 item 4 exige *"a primeira sessão depois que o leitor renderizar um livro ponta a ponta"* — **nenhum livro foi renderizado** (a T13 não rodou), então a remoção física continua **não devida**. O que aconteceu foi mais fraco e está dito como tal: o chat **perdeu a porta** (AD-056), e duas deleções foram **forçadas pelo compilador**, não escolhidas — `src/components/Sidebar/ChatList.tsx` e `src/components/Documents/DocumentsPanel.tsx`, ambos comparando contra `"chat"` numa união que deixou de ter esse membro | **parcialmente registrado.** Anotado em `chat-messaging/spec.md`, `conversation-memory/spec.md` e `documents-rag/spec.md` (banner + linha a linha, **sem apagar requisito nenhum**) e na AD-056. **Pendente:** o gatilho em si, que só a T13 dispara, e o todo de remoção que ele torna devido |
| READ-20 | P1: Traduzir pelo sidecar llama.cpp local, sem rede | T6: `translate_paragraph` conhece um único caminho, `LlamaServerClient::stream_chat` acumulado, e `process_book` monta o cliente por `runtime_commands::client` | **verificado só por leitura e compilação** — não há import de HTTP nem URL nesta rota, e o cliente aponta para `127.0.0.1:<porta do sidecar>`. **Nenhum teste chama `translate_paragraph`**: todos injetam um duble, e nenhum sobe o `llama-server` ou toca a rede. A prova de que a ponte Rust→sidecar funciona continua sendo a T13 — a T1 mediu o servidor e o protocolo, não este código |
| READ-21 | P1: Modelo default de tradução designado, com oferta de download | T1 escolheu `gguf-qwen2.5-7b` por medição; **T6 escreveu a constante** `translate::DEFAULT_TRANSLATION_MODEL` e a regra de seleção `select_model` | **metade verificada** — 2 testes em 249/0/16: `the_default_translation_model_is_a_curated_catalog_id` (a constante casa com um `id` de `CURATED_MODELS`, senão a oferta apontaria para o nada) e `translation_without_an_active_model_names_the_default_instead_of_failing_blank` (a mensagem de erro carrega o id; com modelo ativo, é ele que sai). pending — **a oferta de download é o diálogo, T10/T15**: nada nesta task chama `download_model`, e nenhum código de download foi escrito |
| READ-22 | P1: Um parágrafo por requisição, nunca a página inteira | T4 deu a fundação (`split_paragraphs`, uma definição de parágrafo para o app todo); **T6 fecha**: `translate_page` reusa a mesma função e faz uma chamada por parágrafo | **verificado, com duble que registra o que recebeu** — `each_request_carries_exactly_one_paragraph` em 249/0/16: uma página de 4 parágrafos produz **exatamente 4** chamadas, nenhuma contém `\n\n`, e o conteúdo de cada uma é conferido item a item. Some-se `an_empty_paragraph_translation_fails_the_page_instead_of_dropping_it`, que prova que o laço para na falha em vez de seguir pedindo. **Nenhuma requisição real foi feita** — o duble é o único "modelo" que estes testes conhecem |
| READ-23 | P1: Remontar a página preservando as quebras de parágrafo | T6 escreveu o remontador em `translate_page`: cada parágrafo aparado e juntado por `\n\n` | **verificado** — `the_page_is_reassembled_with_its_paragraph_breaks` em 249/0/16: 3 parágrafos traduzidos voltam com a string exata esperada **e** `split_paragraphs` os relê como 3, não como um bloco. A aparadura casa com o que `storage::write_pages` deixa em `original/`: sem separador no fim, o arquivo é exatamente a página |
| READ-24 | P1: Painel de edição do livro (idioma, modelo, páginas) | **T15** escreveu `src/components/Library/BookEditPanel.tsx` (novo, 3 seções: modelo, idiomas, páginas), montado pela `LibraryPanel` **embaixo da própria linha** do livro | **não verificado — só compila.** `npm run build` **exit 0** (1.861 módulos, 2,86 s, `tsc` limpo). **Nada foi visto na tela, nenhum `invoke` foi disparado** e não há suíte de frontend. Comportamento é T13 |
| READ-25 | P1: Escolher o modelo pelo painel, reusando `set_active_model` | **T15**: `<select>` alimentado por `runtimeStore.installedModels` e aplicado por `setActiveModel` — **nenhum comando novo**; o aviso de reinício (`library.editModelRestart`) fica sob o seletor; com zero instalados, o painel nomeia `DEFAULT_TRANSLATION_MODEL` e monta o mesmo `ModelDownloadCard` da tela de Runtime | **não verificado — só compila.** `npm run build` **exit 0** (1.861 módulos, 2,86 s, `tsc` limpo). **Nada foi visto na tela, nenhum `invoke` foi disparado** e não há suíte de frontend. Comportamento é T13 |
| READ-26 | P1: Retraduzir apenas as páginas selecionadas, sem repaginar | T14 escreveu `reader_commands::mark_for_retranslation` + comando `retranslate_pages` (`pages` **base 0**, `None` = o idioma inteiro), que apaga o arquivo da página e chama o mesmo laço da T6 | **backend verificado** — `retranslating_one_page_deletes_only_that_file` (265/0/16): num livro de 10 páginas, pedir a página 3 (base 0) some **só** com `pt/0004.txt`, os outros 9 continuam lá e `next_missing` devolve 4. `retranslating_pages_never_changes_page_count_or_the_original_files`: `page_count` continua 10 e os 10 `original/*.txt` são byte a byte os mesmos; página além do fim é recusada e `original/` não é apagável por este caminho. **Não verificado:** nada foi retraduzido de verdade — nenhum modelo rodou, nenhum comando Tauri foi invocado, e a tela que seleciona as páginas é T15 · **T15:** a seleção existe na tela — uma grade de botões numerados (base 1 no rótulo, **base 0 no `invoke`**) dentro de um bloco rolável, e o botão de retraduzir só habilita com pelo menos uma página marcada; o **reprocessar o livro inteiro** é botão separado, chamando `process_book`. **Só compila** — nenhuma página foi retraduzida pela tela, e conferir no disco que só um arquivo mudou de data é T13 (item 12) |
| READ-27 | P1: Trocar o idioma de leitura não apaga arquivo nenhum | T14 escreveu `set_book_reading_language` + comando `set_reading_language` — só `UPDATE books SET reading_language`, nenhum `remove_*` | **backend verificado** — `changing_the_reading_language_deletes_nothing` (265/0/16): com `original/`, `pt/` e `en/` cheios (30 arquivos), trocar pt→en→pt→original deixa os 30 arquivos byte a byte iguais. `original` e `None` gravam NULL, uma representação só. **Não verificado:** o leitor passando a ler da outra pasta ponta a ponta é T9/T15 — nada aqui abre o app · **T15:** trocar o idioma de leitura é um botão por idioma que chama `set_reading_language`, e a lista inclui o **original** como opção. **Nenhum `remove_*` é chamado por esse caminho** — conferido lendo o componente, não presumido. **Só compila**; que nada suma ao trocar é T13 |
| READ-28 | P1: Traduções de vários idiomas coexistem, uma pasta cada | T16 escreveu `reader::storage::{book_dir, lang_dir}` | **verificado como mecanismo** — `two_language_folders_coexist_in_the_same_book_folder` (220/0/15): `original/`, `pt/` e `en/` lado a lado com 10 arquivos cada, e a página 6 é a mesma nos dois idiomas. **Nenhuma tradução real foi gravada** e nada chama isso ainda — T6/T14 · **T14:** `retranslating_one_language_leaves_the_other_untouched` — apagar **todas** as páginas de `pt/` deixa os 10 arquivos de `en/` byte a byte iguais e `original/` com 10; e `book_languages` lista as três pastas lado a lado. Continua **sem tradução real** e sem app aberto · **T9:** o `<select>` do cabeçalho lista o retorno de `list_book_languages` (inclui `original`) e chama `setLanguage`, que mantém a página corrente. **Só compila** — o seletor nunca foi aberto e nenhum idioma foi trocado (T13) · **T11:** com a rota, o `<select>` do `ReaderPanel` passa a ser alcançável pela primeira vez. **Continua sem nunca ter sido aberto** (T13) |
| READ-29 | P1: Remover um idioma apaga só a pasta dele, com contagem antes | T16 escreveu `reader::storage::remove_lang` | **metade verificada** — `remove_lang_deletes_one_folder_and_leaves_the_others`: apagar `pt/` deixa `en/`, `original/` e o arquivo importado intactos, e apagar duas vezes não é erro. **A contagem antes de aplicar é UI (T15)** e não tem prova nenhuma; o teste só mostra que `translated_pages` responde a contagem antes do `remove_lang` · **T14 fechou a contagem no backend:** `remove_book_language` conta as páginas **antes** do `remove_lang` e **devolve** o número — `removing_a_language_deletes_only_its_folder_and_reports_the_count_first` afirma `lost == 10`, `en/`+`original/` intactos, o `.epub` importado no lugar, remover de novo = 0 e `original` recusado. `removing_the_language_being_read_falls_back_to_the_original` prova que a coluna volta a NULL só quando o idioma removido era o que estava sendo lido. **Mostrar essa contagem e pedir confirmação antes continua sendo T15, sem prova nenhuma** · **T15:** a confirmação existe e **carrega a contagem na própria pergunta** — `library.editRemoveConfirm` interpola `{{language}}` e `{{pages}}`, com `pages` vindo de `list_book_languages` (contado do disco). Usa `window.confirm`, o mesmo caminho que o `UpdateBanner` já usa nesta base. **Só compila** — o diálogo nunca apareceu, e ninguém confirmou nem cancelou nada (T13, item 14) |
| READ-30 | P1: Progresso e reprocessamento seletivo são por idioma | T16 deu a fundação (`translated_pages`/`next_missing` contam do disco, por pasta); **T6 fez do idioma um parâmetro**: `translate_book(..., language: Option<&str>, ...)` e `process_book(app, book_id, language)`, sem coluna de estado de tradução | **metade verificada** — o idioma é parâmetro em todos os 9 testes de laço (249/0/16), e o progresso emitido no `book-status` carrega `language: Some(...)` com `done` contado da pasta daquele idioma. `asking_for_no_translation_never_touches_the_model` prova que `None` não pede nada. pending — **quem exibe progresso por idioma é T14/T15, e quem reprocessa é T14**; o evento nunca chegou ao frontend · **T14:** `list_book_languages` devolve `BookLanguage { language, pages, reading }` com `pages` contado do disco — `listing_languages_counts_pages_from_disk_not_from_the_database` apaga `pt/0004.txt` **por fora** e a contagem cai de 10 para 9 enquanto `page_count` continua 10. `retranslate_pages`/`add_language`/`remove_language` recebem o idioma como parâmetro, e nenhum deles escreve estado de tradução. **Quem exibe continua sendo T15**, e o evento segue sem nunca ter chegado ao frontend · **T15:** o painel lista **uma linha por idioma com a sua contagem**, mais a linha do `original` com `page_count`, e o progresso durante uma rodada vem do mesmo `book-status` (`library.translating` com `done/total`). **Só compila**; o evento continua sem nunca ter chegado a esta tela |
| READ-31 | P1: Texto em arquivos, um por página, em pasta por idioma | T16 escreveu `reader::storage::{page_file, write_pages, read_page, translated_pages, next_missing}` | **verificado como função pura, contra pasta temporária** — 5 testes em 220/0/15: `page_files_are_zero_padded_so_the_explorer_sorts_them_in_reading_order` (`0010.txt` depois de `0002.txt` na ordenação alfabética), `write_pages_clears_the_folder_first_so_a_shorter_reprocess_leaves_no_leftovers` (10 → 5 páginas não deixa `0006..0010`), `next_missing_finds_the_first_gap_not_the_first_absent_at_the_end`, `next_missing_returns_none_when_the_language_is_complete` e, para o **critério 9**, `a_file_deleted_by_hand_makes_that_page_pending_again` (apagar `pt/0003.txt` por fora faz `next_missing` devolver 3, sem refazer as vizinhas). **Nenhum texto de livro real foi gravado** e ninguém chama `storage` ainda — ponta a ponta é T5/T13 |
| READ-32 | P1: Livro em pasta própria, com migração do layout antigo | T17 escreveu `folder_for`, o novo destino de `import_books` e `migrate_layout`/`migrate_legacy_layout`, chamada no `setup` ao lado de `requeue_unfinished_documents` | **verificado contra pasta temporária + banco em memória** — 7 testes em **227/0/16**: `importing_puts_the_file_inside_a_folder_of_its_own` (32.1), `two_books_that_would_share_a_folder_name_get_a_suffix` (32.2, `a.pdf`→`a/`, `a.epub`→`a (2)/`), `the_layout_migration_moves_a_loose_file_into_its_folder_and_records_it` (32.3), `running_the_layout_migration_twice_moves_nothing_the_second_time` (32.4, com `original/0001.txt` de sentinela), `a_file_that_cannot_be_moved_keeps_its_row_and_its_file` (32.5, **falha injetada** — o próprio teste diz que é inconclusivo quanto à causa real), `a_row_whose_file_is_already_gone_is_skipped_without_failing_the_boot` e `removing_a_book_deletes_its_folder_and_everything_in_it`. **NADA foi exercitado contra biblioteca real:** o teste `#[ignore] migrate_legacy_layout_against_a_real_library_copy` existe e lê o caminho de `READER_LEGACY_LIBRARY`, mas a variável não estava definida e ele **não rodou** — o ensaio que o `AGENTS.md` exige antes de uma migração destrutiva continua **em aberto**. `migrate_legacy_layout` também nunca rodou no boot de verdade: não há runner de integração Tauri |
| READ-33 | P1: Seletor de idioma no leitor só quando o livro tem mais de um idioma (AD-070) | `ReaderPanel.tsx` — `languages.length > 1` | implemented — **sem teste** (sem suíte de frontend); `npm run build` exit 0. Não visto na tela |
| HIST-01 | P1: A lateral lista leituras, não conversas (spec `reading-history`) | pending | **desbloqueado por esta spec** |
| HIST-02 | P1: Ordenação por abertura mais recente (spec `reading-history`) | T7 escreveu `reading_history`: `WHERE last_opened_at IS NOT NULL ORDER BY last_opened_at DESC` | **verificado como SQL** — `the_history_lists_the_most_recently_opened_first` em **257/0/16**, com timestamps escritos à mão (duas aberturas na mesma execução podem colidir — precedente da `book-library`). pending — **a lateral é T11** |
| HIST-03 | P1: Estado vazio apontando para a Biblioteca (spec `reading-history`) | pending | **desbloqueado por esta spec** |
| HIST-04 | P1: Registrar a última abertura (spec `reading-history`) | T7: `open_position` grava `Utc::now().to_rfc3339()` em `last_opened_at` | **verificado em unidade, com ressalva escrita no teste** — `opening_a_book_records_the_moment_it_was_opened` (257/0/16) prova que a coluna deixa de ser nula e que o valor relê como RFC 3339; **que o instante seja o certo é fé no relógio do sistema** |
| HIST-05 | P1: Persistir a posição durante a leitura (spec `reading-history`) | T7: `save_position` — mesma prova do READ-17 | **metade verificada (backend)** — `saving_a_position_persists_it` (257/0/16). pending — **quem persiste enquanto a leitura avança é a T8/T9** · **T8 escreveu a metade dela**: gravação debounced (800 ms) no `readerStore`, com *flush* ao fechar. Só compila — `npm run build` exit 0; nenhuma gravação real aconteceu · **T9:** o `ReaderPanel` é quem chama `goToPage` (clique e setas), a única porta do debounce. **Nunca disparou** — o componente não está roteado (T11) e não há suíte de frontend |
| HIST-06 | P1: Reabrir na posição salva (spec `reading-history`) | T7: `open_position` devolve `last_page` clampado — mesma prova do READ-16 | **metade verificada (backend)** — `reopening_a_book_returns_the_saved_page` (257/0/16). pending — **reabrir pela lateral é T11/T13** · **T8:** `readerApi.listReadingHistory` e `readerStore.openBook` prontos para a lateral consumir; a lateral não existe · **T9:** o painel exibe a página que o store trouxe do `open_book`, mas **não abre livro nenhum**: quem chama `openBook` é a lateral (T11). Nada foi reaberto de verdade |
| HIST-07 | P1: Livro nunca aberto começa na primeira página (spec `reading-history`) | T7: `last_page` NULL → 0, sem gravar posição na abertura | **verificado em unidade** — `a_book_never_opened_starts_at_the_first_page` (257/0/16): devolve 0 **e a coluna continua NULL**, porque quem marca "foi lido" é `last_opened_at`. Some-se `an_imported_book_never_opened_is_not_in_the_history` |
| HIST-08 | P1: Remover o livro remove páginas e histórico (spec `reading-history`) | T16 + T17 + T5, pela mesma rota do READ-18 | **verificado em unidade** — `removing_a_book_removes_its_whole_folder` (238/0/16). O histórico vive nas colunas da própria linha de `books`, que o `DELETE` já apaga; pela tela, é a T13 |

**ID format:** `READ-[NUMBER]`
**Status values:** Pending → In Design → In Tasks → Implemented → Verified
**Coverage:** 32 requisitos próprios + os 8 `HIST-xx` que esta feature passa a implementar.

⚠️ **A armadilha desta base, repetida aqui porque ela vale para cada task:** `src/types.ts` é escrito **à mão** e **não há gate nenhum sobre ele** (AD-054). `BookPage`, `BookRecord` e os eventos de progresso cruzam a fronteira Rust↔TS; uma divergência de campo deixa `cargo check` **e** `npm run build` os dois limpos e ninguém avisa. Conferência campo a campo é obrigatória e é humana.
