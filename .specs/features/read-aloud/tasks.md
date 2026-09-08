# Leitura em voz alta — Tasks

**Spec:** `.specs/features/read-aloud/spec.md`
**Design:** `.specs/features/read-aloud/design.md`
**Status:** T0..T7 **executadas** em 2026-09-07. **T8 parcialmente executada** em 2026-09-08 — a
leitura foi exercitada no app rodando, com a voz `pt_BR-faber-medium` já instalada, e os itens 1..2
e 4 estão cobertos (ver T8). Os itens 3, 5, 6 e 7 continuam por fazer.

**O primeiro play falhou.** `speaker::start` punha o filho num Job Object criado inline —
`JobState::create().assign(&child)` — e `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` mata tudo dentro do job
quando o **último handle fecha**, o que acontecia no fim daquela mesma linha. O piper morria antes de
imprimir o primeiro caminho, e a tela dizia "o leitor de voz encerrou sem responder". Corrigido com
um `OnceLock<JobState>` de processo (AD-062, L-011). Regressão coberta por
`speaker::tests::the_job_outlives_the_statement_that_assigns_the_child`, verificada por mutação: com
a linha antiga de volta, o teste **falha**.

**Medido depois de implementar:**

| | |
| --- | --- |
| `cargo test --lib` | **310 / 0 falhas / 18 ignorados** (era 294; +16 testes novos) |
| `npm run build` | exit 0 |
| `npm run test:scripts` | 49 |
| i18n | 229/229 nos dois idiomas |
| `resources/` depois do vendoring | **184,7 MB** (piper = **28,6 MB**, já sem os 10.261.536 bytes do árabe) |
| Binário podado, do bundle, **executado** | exit 0, WAV de 69.280 bytes em 1.102 ms |

---

## Test Coverage Matrix

Vale a matriz de `.specs/codebase/TESTING.md`: função pura em Rust é teste obrigatório; comando Tauri
que só orquestra I/O **não tem runner de integração**; componente React **não tem suíte**.

| Requisito | Tipo de prova | Onde |
| --- | --- | --- |
| TTS-01 | `npm run vendor` traz o binário, e ele **executa** podado | T6 |
| TTS-02 | unit — a página vira frases, na ordem, pela fronteira da paginação | T2 |
| TTS-03 | **nenhum** — marcar na tela é UAT. Lacuna, não cobertura | T7, T8 |
| TTS-05, TTS-07, TTS-16..TTS-19 | **nenhum** — orquestração de áudio no frontend | T7, T8 |
| TTS-08 | unit — bloco sem texto não vira fala | T2 |
| TTS-09, TTS-10, TTS-11, TTS-36 | unit — corpus de 13 formas de executar código | T1 |
| TTS-12 | **nenhum** — `sandbox="allow-scripts"` é uma linha de JSX | T7, T8 |
| TTS-13, TTS-24 | unit — as duas recusas, nomeando componente e ação | T3 |
| TTS-14 | por leitura — o WAV é apagado após ser lido; nada vai para a pasta do livro | T3 |
| TTS-20, TTS-22, TTS-26, TTS-31 | unit — catálogo, meia-voz não instalada, `resolve`, espaço liberado | T4 |
| TTS-21 | **nenhum** — o download real é UAT | T5, T8 |
| TTS-35 | **nenhum** — depende das vozes do SO | T7, T8 |

---

## Execution Plan

| Fase | Tasks | Por quê juntas |
| --- | --- | --- |
| 0 — medir | T0 | Nada podia ser desenhado antes de saber se o Piper aguenta uma frase por vez |
| 1 — segurança | T1 | Fecha a porta **antes** de o sandbox abrir. Não pode vir depois |
| 2 — puro | T2, T4 | Funções sobre `&str` e sobre `&Path`, sem processo e sem tela |
| 3 — processo | T3, T5, T6 | O sidecar, a fronteira e o componente no instalador |
| 4 — tela | T7 | Precisa dos comandos |
| 5 — prova | T8 | Precisa de tudo, e do usuário |

---

## Gate Check Commands

```bash
cd src-tauri && cargo test --lib
cd src-tauri && cargo check --lib
npm run build
npm run test:scripts
npm run vendor
npm run tauri dev
```

---

## Task Breakdown

### T0: spike — medir o Piper antes de desenhar ✅

**Depends on:** nada
**Tests:** nenhum automatizado — é medição manual do binário, e os números estão no `design.md`.
**Gate:** o binário roda e o laço de stdin é confirmado.

Binário arquivado baixado e executado: 593 ms frio, 1.536 ms para 5 frases num processo, ~236 ms
marginais, síntese ~10× o tempo real, WAV 22.050 Hz mono 16 bits. **Confirmou o laço de stdin**, que é
o que decidiu o sidecar de vida longa. Os números estão no topo do `design.md`.

### T1: `reader/sanitize.rs` — fechar a porta antes de abri-la ✅

**Depends on:** T0
**Gate:** `cargo test --lib` acima do baseline; `cargo check --lib` sem warnings.

**Files:** `src-tauri/src/reader/sanitize.rs` *(novo)*, `reader/epub.rs`, `reader/mod.rs`

Remove `<script>` em qualquer profundidade, atributos `on*`, URLs `javascript:`/`vbscript:`/
`data:text/html`, e `<iframe>`/`<object>`/`<embed>`. Substitui a linha de `epub.rs` que só pegava
`<script>` de primeiro nível. Roda na **extração**, então o `.html` em disco é o artefato auditado.

**Dois defeitos reais achados pelos próprios testes**, e é a razão de a task existir antes das outras:

1. um `<` solto (`5 < 7`) saía duplicado — o laço quebrava sem avançar o cursor;
2. `java&#09;script:` **passava**. A primeira versão filtrava espaço e controle sem decodificar
   entidade. A correção lê o esquema **mantendo só as letras** antes do `:`, o que colapsa
   `JaVaScRiPt:`, `java\tscript:` e `java&#09;script:` na mesma string sem decodificar nada.

**Tests:** corpus de 13 formas de executar código + um teste que **exige** que a formatação não mude +
um que garante que link legítimo com `:` sobrevive (o risco da correção era virar "bloqueia tudo").

### T2: `tts/timing.rs` — a página vira frases ✅

**Depends on:** nada
**Tests:** ordem de leitura; bloco sem texto não vira fala; marcação nunca chega ao sintetizador; a fronteira de frase é a mesma da paginação.
**Gate:** `cargo test --lib`.

`utterances()` e `split_sentences()`, apoiadas em `pagination::sentence_starts` — **a** definição de
frase do app, agora `pub(crate)`. Um teste compara as duas para travá-las na mesma resposta.

Bloco sem texto visível não vira fala, o que faz a TTS-08 verdadeira por construção.

**Removido durante a execução:** `wav_duration_ms`, que lia a duração do cabeçalho do WAV. Correta e
redundante — o `<audio>` que toca o arquivo já sabe a própria duração e dispara `ended`. Dois relógios
podem discordar; um não. Os 2 testes dela saíram junto, com a justificativa que o `AGENTS.md` exige.

### T3: `tts/speaker.rs` — o sidecar ✅

**Depends on:** T0, T4
**Gate:** `cargo test --lib`; `cargo check --lib` sem warnings.

Um `piper.exe` vivo por voz, alimentado linha a linha pelo stdin. Reusa `configure_command`
(`CREATE_NO_WINDOW`, SIDE-03) e o Job Object — as duas mortes. O WAV é lido e **apagado** na hora
(TTS-14).

**Tests:** só as duas recusas afirmáveis sem o binário — componente ausente e voz ausente. O caminho
feliz é UAT, e está dito no cabeçalho do arquivo.

### T4: `tts/voices.rs` — catálogo, disco e escolha ✅

**Depends on:** nada
**Tests:** voz pela metade não conta como instalada; `resolve` com e sem escolha; remover informa o espaço e é idempotente; todo item do catálogo é endereçável.
**Gate:** `cargo test --lib`.

Catálogo curado com 4 vozes (pt-BR, en-US, en-GB, es-ES). **Só a pt-BR tem tamanho medido**
(63.201.294 bytes, do spike); as outras estão em `0`, que a tela mostra como *desconhecido* —
inventar número que o usuário lê como fato é pior que não ter número.

`installed()` só conta voz com **os dois** arquivos, que é o que impede um download pela metade de
ser escolhido depois. `resolve()` decide a voz: escolha do usuário, depois qualquer voz do idioma,
depois `None` — e `None` é o gancho do fallback do sistema, não uma falha.

### T5: `tts_commands.rs` ✅

**Depends on:** T3, T4
**Tests:** nenhum — comando Tauri não tem runner de integração neste projeto. **Lacuna registrada, não cobertura.**
**Gate:** `cargo check --lib` sem warnings.

7 comandos. O download grava em `.part` e **renomeia no fim**; progresso por evento
`voice-download-progress`, o mesmo formato do download de modelo (AD-018). O WAV volta por
`ipc::Response`, o caminho de `get_book_image`.

### T6: vendoring do Piper ✅

**Depends on:** T0
**Tests:** os 49 de `npm run test:scripts` continuam passando.
**Gate:** `npm run vendor` traz o componente **e o binário podado executa** — a lição da AD-046.

Entrada em `scripts/vendor.json` com asset e bytes fixados, e uma poda **por nome exato** do
`libtashkeel_model.ort`. A poda é nomeada em vez de casada por padrão porque a AD-046 registra uma
poda por padrão que quebrou o bundle — e por isso o binário podado foi **executado**, não só
inspecionado.

### T7: a tela ✅

**Depends on:** T2, T5
**Tests:** nenhum — não há suíte de frontend nesta árvore. **Lacuna registrada, não cobertura.**
**Gate:** `npm run build` exit 0; paridade de i18n.

`readerScript.ts` é o único script que roda dentro do iframe: marca a frase com a **CSS Custom
Highlight API** (pinta um `Range` sem inserir elemento — um `<span>` mudaria o DOM do livro e um CSS
que mira `p > em:first-child` mudaria de aparência ao ser lido) e devolve o clique por `postMessage`.
`readAloudStore.ts` orquestra: uma frase de antecipação, avanço de página, clique para começar dali,
e o `speechSynthesis` quando não há voz instalada. Botão no cabeçalho e barra de espaço.

### T8: UAT — **PARCIAL** (1, 2, 4 feitos em 2026-09-08; 3, 5, 6, 7 pendentes)

**Depends on:** T7
**Tests:** nenhum automatizado — é UAT manual, com voz e livro reais.
**Gate:** nenhum automatizado. É a task que decide se a feature existe.

1. ~~Baixar uma voz pelo catálogo~~ e ouvir uma página. **Ouvir: feito** com
   `pt_BR-faber-medium.onnx`, que já estava instalada. **O download pelo catálogo continua sem prova.**
2. Conferir que a marcação acompanha a frase, e **medir a deriva** — é o número que decide se a
   marcação por palavra (TTS-37) vale a pena. **Acompanha: feito.** Duas capturas da página 7,
   separadas por 10 s, mostram o destaque na 3ª frase e depois na 7ª. **A deriva não foi medida** —
   capturas de tela não a medem, e sem ela TTS-37 continua sem decisão.
3. Clicar num parágrafo anterior e confirmar que recomeça dali. **Não feito.**
4. Deixar virar de página sozinho. **Avanço entre frases: feito** (item 2). **A virada de página em
   si não foi vista** — a página 7 não chegou ao fim nos 15 s observados.
5. Sem voz instalada, confirmar que o `speechSynthesis` assume e que a tela diz isso. **Não feito.**
6. Processar um EPUB com `<script>` aninhado, `onerror` e `javascript:` e conferir o `.html` em disco.
   **Não feito** — o sanitizador tem testes unitários, mas nenhum livro de verdade passou por ele.
7. Renomear a pasta do piper e confirmar que o livro continua legível. **Não feito.**
