# Leitura em voz alta com marcação palavra a palavra — Specification

**Milestone:** M10.3
**Estende:** `.specs/features/epub-fidelity/` — o leitor, o iframe e a página em blocos já existem
**Revoga em parte:** `.specs/features/epub-fidelity/` FID-04 (isolamento sem script). Ver a seção final.

## Problem Statement

O leitor mostra a página e não faz mais nada com ela. Quem quer ouvir o livro precisa de outro
programa, e aí perde a tradução, a posição de leitura e a formatação que a `epub-fidelity` acabou de
entregar. O pedido do usuário foi literal: *"usar a IA para ler a página e ele ir marcando onde está
lendo, como se fosse um karaokê"*.

Duas correções de premissa, medidas antes de escrever esta spec:

1. **Não é o modelo que lê.** O llama.cpp embutido gera texto, não áudio. A voz é um componente novo.
2. **O Piper não entrega tempo por palavra.** O flag `--alignment-data`, que produziria os tempos,
   é o PR #407 do `rhasspy/piper` — **aberto, nunca mergeado**, e só funciona no script Python, não
   no executável. Um mantenedor registrou que a solução real exigiria reexportar todas as vozes ONNX.
3. **O projeto vivo do Piper não publica executável.** `OHF-Voice/piper1-gpl` v1.8.0 (jul/2026,
   GPL-3.0) tem **só wheels Python** — o de Windows pesa 34.119.688 bytes. Usá-lo significaria
   embutir um runtime Python, classe de dependência que este repositório evitou de propósito.
   Quem publica binário nativo é o `rhasspy/piper` **arquivado** (release `2023.11.14-2`,
   `piper_windows_amd64.zip` = 22.477.236 bytes, `piper_linux_x86_64.tar.gz` = 26.460.462 bytes), e
   ele é **MIT**. O caminho vivo custa mais e obriga a GPL; o caminho congelado é mais barato nos
   dois eixos.

## Goals

- Ouvir o livro correndo sozinho, de página em página, na voz do Piper, sem rede e sem sair do app.
- Ver a palavra corrente marcada enquanto o áudio corre.
- Clicar numa palavra e a leitura recomeçar dali — para voltar um parágrafo sem procurar o começo.
- Escolher **qual voz** e **qual idioma**, de uma lista, sem que o instalador engorde por idioma
  que ninguém usa.
- **Experimentar antes de decidir**: ouvir uma voz e ver um modelo trabalhando no próprio livro,
  em vez de escolher pelo nome.
- Não perder nada do que a `epub-fidelity` entregou: fidelidade, tradução, posição de leitura.
- O livro continuar legível e utilizável se o áudio falhar.

## Out of Scope

| Fora | Por quê |
| --- | --- |
| Tempo de palavra **medido** | O Piper não expõe. Alinhamento forçado (whisper.cpp) daria o tempo real e foi **descartado pelo usuário em 2026-09-07**: dois binários novos no instalador e um passo de processamento por página |
| Vozes do sistema (`speechSynthesis`) | Descartado pelo usuário na mesma decisão. Daria tempo por palavra de graça, mas a voz varia de máquina para máquina |
| Marcador de áudio persistente ("continuar de onde o áudio parou" ao reabrir o livro) | A posição salva é a **página** (HIST-05), e continua sendo. Reabrir o livro volta ao começo da página, e o clique resolve o resto |
| Gravar o áudio em arquivo / exportar audiobook | Ninguém pediu, e mudaria o layout em disco do livro |
| Destaque de frase **na tradução enquanto ela é gerada** | A tradução tem o seu próprio fluxo (READ-12). Ler em voz alta lê o que está na tela |
| Rolagem automática acompanhando a marcação | Ninguém pediu. A palavra fica marcada; rolar é do usuário |
| Comparar duas vozes ou dois modelos **lado a lado, ao mesmo tempo** | Testar um de cada vez responde a pergunta. Uma tela de comparação simultânea é produto novo |
| Nota / pontuação automática de qualidade de voz ou de tradução | Quem julga é o ouvido e o olho do usuário. Uma métrica automática aqui seria inventada |
| Treinar voz, clonar voz, voz do próprio usuário | Outra ordem de grandeza: exige treino, dados de áudio e uma interface inteira. O catálogo cobre o pedido |
| Traduzir para um idioma **só para poder ouvi-lo** | Ouvir lê o que está na tela. Traduzir continua sendo o fluxo da READ-30, disparado pelo usuário |

## Assumptions & Open Questions

| Assumption / decision | Chosen default | Rationale | Confirmed? |
| --- | --- | --- | --- |
| Fonte da voz | `rhasspy/piper`, release `2023.11.14-2`, binário nativo vendorizado como o llama.cpp, o pdfium e o ONNX Runtime | Decidido pelo usuário em 2026-09-07 **depois** de medir as duas opções. Encaixa sem adaptação em `scripts/vendor.json` + `bundled::find_file`, e o Windows pesa **22.477.236 bytes** — número lido da API do GitHub, não estimado | y |
| Licença | **MIT.** A GPL foi evitada, não aceita | O `piper1-gpl` é GPL-3.0 porque embute o espeak-ng, e só existe como wheel Python. Escolher o binário arquivado troca uma obrigação de GPL e um runtime Python por uma linha de atribuição MIT. **A atribuição continua sendo requisito** (TTS-15) — MIT obriga a acompanhar o aviso de copyright | y |
| O projeto está arquivado — e daí? | Risco aceito, com saída escrita | `rhasspy/piper` foi arquivado em out/2025 e o release é de nov/2023: nenhuma correção de bug virá. O que reduz o risco é o formato da voz ser `.onnx` + `.json` — dados, não código — então trocar o motor depois não invalida as vozes baixadas. A saída, se o binário quebrar num Windows futuro, é o `speechSynthesis`, que continua disponível e já foi desenhado nesta conversa | y |
| Unidade da marcação na v1 | **Frase**, não palavra | **Mudado pelo council em 2026-09-07.** A duração da frase é *exata* (sai do tamanho do WAV); a da palavra seria interpolada por peso de caracteres, e duas das três vozes chamaram isso de precisão fingida — "1999" tem 4 caracteres e cinco sílabas, e em CJK não há espaço nenhum. Frase entrega o karaokê com tempo medido; palavra entra na P2 **condicionada à medição da deriva** num livro real | y |
| Se nenhuma voz do Piper estiver instalada | Cai no `speechSynthesis` do sistema | **Trazido pelo council.** Resolve o buraco da primeira execução — sem ele, ouvir qualquer coisa custa um download de 63 MB antes do primeiro som. Zero MB, e o `boundary` do próprio navegador dá a marcação | y |
| Como o usuário aciona a leitura | Botão no cabeçalho do leitor, barra de espaço, e o clique numa palavra | Lacuna encontrada pelo usuário em 2026-09-07: a spec dizia "WHEN the user starts read-aloud" sem dizer como. As setas já são página no `ReaderPanel`, então espaço é a tecla livre e é a convenção de todo leitor | y |
| Isolamento do iframe | `sandbox="allow-scripts"` sem `allow-same-origin` | Decidido pelo usuário em 2026-09-07. Marcar a palavra exige script dentro da página, e a origem continua opaca — o script do app não alcança o app, e o `data:` das imagens continua funcionando | y |
| O que fazer com o JS do livro | Sanitizar na **extração**, não na exibição | Com `allow-scripts` o `<script>` do livro rodaria. Hoje a extração só derruba `<script>` de primeiro nível: um aninhado num `<div>` sobrevive, e `onclick=` e `href="javascript:"` nunca foram tocados. Sanitizar na extração grava o resultado em disco, então é auditável no explorador — o que a READ-31 já promete | y |
| Voz é **componente** ou **modelo**? | **Modelo.** O binário do Piper vai no instalador; as vozes vêm de um catálogo curado e são baixadas sob demanda | Achado lendo o repositório, não suposto: a SELF-10 registra que componente de runtime **nunca** baixa (`prepare_runtime` não faz HTTP; `runtime/release.rs` foi apagado), enquanto os modelos GGUF já têm catálogo curado e `download_with_progress`, e a faxina da SELF-18 **preserva os modelos**. Voz cabe exatamente no segundo molde, e é o que permite ter idiomas sem inchar o instalador | y |
| Alguma voz acompanha o instalador? | **Nenhuma.** Na primeira leitura o app nomeia a ação: baixar uma voz | É a mesma forma que a tradução já tem (`select_model`: *"Nenhum modelo ativo para traduzir. Baixe ou ative X"*). Embutir uma voz criaria uma exceção à SELF-10 para privilegiar um idioma sobre os outros | y |
| De onde vem a lista de vozes | Do **`voices.json` do próprio piper**, baixado sob demanda e cacheado | A primeira versão era uma lista escrita à mão com 9 entradas — um recorte de quem digitou, e todo idioma que ninguém lembrou simplesmente não existia para o usuário. O manifesto tem **176 vozes em 57 idiomas**, com `size_bytes` por arquivo. Seis ficam embutidas só para a primeira execução sem rede | y |
| Vozes `low`/`x_low` entram? | Só quando o idioma **não tem nada melhor** | Elas são as que soam sintéticas, e não valem ao lado de uma `medium` do mesmo idioma. Mas para um idioma cuja única voz é `low`, descartá-la apagaria o idioma inteiro do app — pior que uma voz áspera | y |
| Quais idiomas entram no catálogo | Os que o Piper publica com voz de qualidade `medium`, começando por pt-BR e en | O app é bilíngue na interface (AD-007), mas o **livro** pode estar em qualquer idioma, e o catálogo é uma lista de dados — crescer é acrescentar linha, não código. **A lista real é montada na T1, com `content-length` conferido por URL**, como o catálogo de modelos já faz | n |
| Idioma lido | Segue o idioma da página na tela; o usuário pode **fixar** outro | O padrão certo é o da página — um livro em inglês lido por voz pt-BR é pior que silêncio. Mas quem lê um original em inglês para treinar o ouvido tem motivo para fixar, e o pedido foi ter a opção | y |
| Onde o áudio é gerado | No backend, por frase, sob demanda | O áudio é grande e temporário. Gerá-lo no Rust deixa o frontend só tocando, e reaproveita o padrão de spawn com Job Object que o sidecar já usa | y |
| O áudio persiste? | Não. Vive em memória/tempo de execução e some ao virar a página | Cachear em disco significaria uma pasta nova por livro, invalidação por retradução, e limpeza. Ninguém pediu, e o custo de gerar de novo é o de uma frase | y |
| Velocidade de leitura ajustável | Fora da P1, entra na P3 | O Piper aceita `--length-scale`, então é uma constante virando um controle. Não é o pedido | y |
| Como se testa uma voz **antes** de baixá-la? | Não se testa: o catálogo mostra idioma, gênero e tamanho, e o teste acontece **depois** de baixar | Piper não publica amostra de áudio por voz num endereço estável que dê para conferir, e baixar uma amostra por voz seria uma segunda rede de downloads para manter. Baixar-testar-remover é o ciclo, e remover está na P2 | y |
| O que o teste de **modelo** traduz? | Um parágrafo da **página aberta**, não uma frase de exemplo | Testar com texto genérico responde a pergunta errada. O usuário quer saber como o modelo se sai **no livro dele** — que é o mesmo princípio da AD-050, medida contra o corpus real | y |
| O teste de modelo grava alguma coisa? | Não. O resultado aparece na tela e é descartado | Gravar viraria uma tradução parcial em disco que a READ-14 leria como página pronta. O teste não pode contaminar o estado do livro | y |

**Open questions:** none — tudo acima está resolvido ou registrado como assumption. Uma **medição**,
não uma questão em aberto, fica para a T1: o tamanho real das duas vozes e o tempo até a primeira
frase soar.

---

## User Stories

### P1: Ouvir a página com a palavra marcada ⭐ MVP

**User Story**: Como leitor, quero ouvir a página aberta com a palavra corrente marcada, para
acompanhar o texto sem lê-lo.

**Why P1**: É o pedido inteiro. Sem áudio não há feature; sem marcação é só um leitor de tela.

**Acceptance Criteria**:

1. WHEN the reader has a page open and the user presses the read-aloud button, THEN the system
   SHALL play that page's audio starting at its first sentence.
2. WHILE audio is playing, the system SHALL mark exactly one sentence as current, and that sentence
   SHALL be the one being spoken.
3. WHEN a sentence finishes, THEN the system SHALL continue with the next sentence of the page
   without user action, moving the mark with it.
3b. WHEN the user presses the space bar while the reader has focus, THEN the system SHALL toggle
    between playing and paused, and SHALL NOT turn the page.
4. WHEN the user stops read-aloud, THEN the system SHALL stop the audio and clear the mark.
5. ⚠️ **ALTERADO em 2026-09-12 (AD-070).** ~~WHEN the user turns the page, changes language, or
   closes the book, THEN the system SHALL stop playback and clear the mark before the new page is
   shown.~~ WHEN the user turns the page or changes language WHILE playing, THEN the system SHALL
   continue reading from the first sentence of the page now on screen; IF playback was paused, THEN
   it SHALL stop and clear the mark. WHEN the user closes the book or opens another one, THEN the
   system SHALL stop playback and clear the mark.
5b. WHEN the user leaves the reader screen (Library, Settings, Runtime) WHILE reading aloud, THEN the
    system SHALL stop playback. (TTS-38)
6. The system SHALL derive the spoken text from the page currently on screen, in the language
   currently on screen.
7. The system SHALL keep every byte on the machine: no network request is made to play a page.
8. IF the user starts read-aloud while it is already playing, THEN the system SHALL keep a single
   playback and SHALL NOT start a second one.
9. WHEN the user pauses, THEN the system SHALL stop the audio and keep the current word marked;
   WHEN the user resumes, THEN playback SHALL continue from the sentence that was interrupted.
10. WHEN the last sentence of a page finishes and a next page exists, THEN the system SHALL turn to
    it and keep playing, without user action.
11. WHEN the last page of the book finishes, THEN the system SHALL stop and clear the mark.
12. WHILE read-aloud is turning pages on its own, the system SHALL save the reading position exactly
    as a manual page turn does (READ-17, HIST-05).

**Independent Test**: abrir um livro, apertar ouvir, e ver a palavra correndo com o áudio até o fim
da página e seguindo para a próxima sozinho; virar a página à mão e confirmar que a leitura continua
do topo da página nova, sem pular página (AD-070); ir para a Biblioteca e confirmar que o áudio parou.

---

### P1: Começar a ler de um ponto escolhido

**User Story**: Como leitor, quero clicar numa palavra e a leitura começar dali, para voltar um
trecho que me escapou sem procurar o começo da página.

**Why P1**: O usuário pediu junto com o karaokê, e pelo motivo dele: *"o usuário pode querer
voltar"*. Sem isto, corrigir o rumo custa parar, virar página e ouvir tudo de novo.

**Acceptance Criteria**:

1. WHEN the user clicks anywhere in the page text, THEN the system SHALL start playback at the
   sentence that contains the click.
2. WHILE audio is playing, WHEN the user clicks another sentence, THEN the system SHALL stop the
   current one and restart at the clicked one.
3. WHEN the user clicks a block that has no readable text, THEN the system SHALL start at the next
   block that has text.
4. The system SHALL accept the click on both render paths: inside the book's iframe and on a plain
   `.txt` page.
5. WHILE read-aloud is stopped, WHEN the user clicks a word, THEN the system SHALL start playing
   rather than only moving the mark.

**Independent Test**: com o áudio correndo no meio da página, clicar num parágrafo anterior e ouvir
a leitura recomeçar exatamente dali.

---

### P1: A página continua sendo a do livro, e nada dela executa

**User Story**: Como leitor, quero que liberar script para o karaokê não deixe o livro rodar código,
para que abrir um EPUB de origem desconhecida continue sendo seguro.

**Why P1**: A `epub-fidelity` comprava esse isolamento com `sandbox=""`. Trocá-lo por
`allow-scripts` sem fechar a porta do outro lado seria uma regressão de segurança, não uma feature.

**Acceptance Criteria**:

1. WHEN a book is processed, THEN the system SHALL remove every `<script>` element from the stored
   pages, at any nesting depth.
2. WHEN a book is processed, THEN the system SHALL remove every event-handler attribute (`on*`)
   from the stored pages.
3. WHEN a book is processed, THEN the system SHALL remove every `javascript:` URL from `href` and
   `src` attributes of the stored pages.
4. The system SHALL render the page in an iframe that has `allow-scripts` and does NOT have
   `allow-same-origin`.
5. The system SHALL keep the page's formatting otherwise untouched: sanitizing removes script, never
   style or structure.
5b. The system SHALL be tested against a corpus that includes, at minimum, a nested `<script>`, an
    `<img onerror>`, an `<svg onload>`, and a `javascript:` href — the four shapes the council named
    as the ones a hand-written sanitizer gets wrong.
6. WHEN a page is displayed, THEN the only script running inside the iframe SHALL be the app's own
   marking script.

**Independent Test**: processar um EPUB com `<script>` aninhado, `onclick=` e `href="javascript:"`,
abrir o `.html` gravado no explorador e não achar nenhum dos três.

---

### P1: O áudio falha sem levar o livro junto

**User Story**: Como leitor, quero que um problema no componente de voz não me impeça de ler,
para que o livro continue servindo para o que serve.

**Why P1**: É a mesma regra que a READ-11 aplica à tradução e a AD-057 ao pdfium.

**Acceptance Criteria**:

1. IF the Piper binary or the voice model is missing, THEN the system SHALL report which component
   is missing and where it was looked for, and the page SHALL stay readable.
2. IF synthesis of a sentence fails, THEN the system SHALL stop playback with a message and SHALL
   NOT leave the page marked.
3. WHILE read-aloud is unavailable, the system SHALL keep every other reader function working:
   turning pages, switching language, translating.
4. The system SHALL write nothing to the user's book folder while reading aloud.

**Independent Test**: renomear a pasta do Piper, abrir o livro, apertar ouvir e ver a mensagem
nomeando o componente — com a página ainda legível e navegável.

---

### P1: Escolher a voz e o idioma

**User Story**: Como leitor, quero escolher entre várias vozes e idiomas, para ouvir numa voz que me
agrade e no idioma que estou lendo.

**Why P1**: Pedido do usuário em 2026-09-07, e é o que decide se a feature serve para mais de um
livro. Sem catálogo o app teria uma voz só, ou um instalador com um idioma que ninguém usa.

**Acceptance Criteria**:

1. The system SHALL offer a curated catalog of voices, each one naming its language and its exact
   download size in bytes.
2. WHEN the user downloads a voice, THEN the system SHALL report progress and SHALL keep the reader
   usable while the download runs.
3. WHERE more than one voice is installed for a language, WHEN the user picks one, THEN the system
   SHALL use it from the next sentence onward.
4. The system SHALL persist the chosen voice per language across restarts.
5. WHILE no voice is installed for the language being read, the system SHALL name the action that is
   missing — download a voice — instead of failing blank, and the page SHALL stay readable.
6. WHEN the user pins a reading language different from the page's, THEN the system SHALL use the
   pinned language's voice until the user unpins it.
7. IF a voice download fails or is interrupted, THEN the system SHALL leave no half-written voice
   that a later read would pick up.

**Independent Test**: com nenhuma voz instalada, apertar ouvir e receber a mensagem nomeando a ação;
baixar duas vozes do mesmo idioma, trocar entre elas e ouvir a diferença na frase seguinte.

---

### P1: Ouvir a voz antes de adotá-la

**User Story**: Como leitor, quero ouvir uma voz instalada dizendo um trecho, para escolher pelo
ouvido em vez de pelo nome do arquivo.

**Why P1**: Pedido do usuário em 2026-09-07 — *"deixe que o usuário teste quais modelos e vozes
quer"*. Sem isto, escolher entre `pt_BR-faber-medium` e `pt_BR-edresson-low` é adivinhação.

**Acceptance Criteria**:

1. WHEN the user asks to test an installed voice, THEN the system SHALL speak a sample and SHALL
   NOT change the voice currently chosen.
2. WHILE a page is open, WHEN the user tests a voice, THEN the sample SHALL be the first sentence of
   that page, so the voice is judged on the book being read.
3. WHILE no page is open, WHEN the user tests a voice, THEN the system SHALL speak a fixed sample
   sentence in that voice's language.
4. WHEN the user tests a voice while read-aloud is playing, THEN the system SHALL stop the playback
   before the sample and SHALL leave the reading stopped afterwards.
5. IF the voice fails to speak the sample, THEN the system SHALL say so next to that voice and SHALL
   leave the other voices usable.

**Independent Test**: com duas vozes pt-BR instaladas, apertar testar em cada uma e ouvir a mesma
frase do livro nas duas antes de escolher.

---

### P2: Experimentar um modelo no próprio livro

**User Story**: Como leitor, quero ver um modelo traduzir um parágrafo do livro que estou lendo,
para escolher o modelo pelo resultado e não pelo tamanho em bilhões de parâmetros.

**Why P2**: Pedido junto com o teste de voz, mas é mais caro: exige uma geração completa e toca a
tradução, que é feature de outra spec. Fica depois da P1 e pode virar spec própria.

**Acceptance Criteria**:

1. WHEN the user tries a model on the open page, THEN the system SHALL translate one paragraph of
   that page with the chosen model and SHALL show the result on screen.
2. The system SHALL write nothing to disk while trying a model: no page file, no translated page.
3. WHEN the trial finishes, THEN the system SHALL report how long it took, so two models can be
   compared on speed as well as on wording.
4. IF the chosen model is not downloaded, THEN the system SHALL name it and offer to download it
   instead of failing blank.
5. WHILE a trial is running, the system SHALL keep the reader usable and SHALL allow cancelling it.

**Independent Test**: com dois modelos baixados, experimentar os dois no mesmo parágrafo e comparar
o texto e o tempo de cada um.

---

### P2: Remover uma voz baixada

**User Story**: Como leitor, quero apagar uma voz que não uso, para recuperar o espaço.

**Why P2**: O catálogo torna fácil acumular vozes. Não é o pedido, mas é a outra ponta dele.

**Acceptance Criteria**:

1. WHEN the user removes a voice, THEN the system SHALL delete its files and SHALL report how much
   space was freed.
2. IF the removed voice was the chosen one for a language, THEN the system SHALL fall back to
   another installed voice of that language, or to none.

**Independent Test**: baixar, remover, e conferir a pasta no explorador.

---

### P3: Ajustar a velocidade

**User Story**: Como leitor, quero mudar a velocidade da leitura, para ouvir do jeito que me agrada.

**Why P3**: O Piper aceita `--length-scale`; é uma constante virando controle. Ninguém pediu.

**Acceptance Criteria**:

1. WHEN the user changes the reading speed, THEN the system SHALL apply it from the next sentence
   onward.
2. The system SHALL persist the chosen speed across restarts.
3. The reader screen SHALL offer the speed control next to the read-aloud buttons, editing the same
   stored value as Settings > Voices. (AD-070)

---

## Edge Cases

- IF a page has no readable text (only a figure), THEN the system SHALL move on to the next page
  instead of playing silence.
- IF the user clicks past the last word with text on the page, THEN the system SHALL start at the
  next page rather than stopping.
- IF a sentence is longer than the synthesizer accepts, THEN the system SHALL split it and play the
  parts in order, with no word left unspoken.
- IF the page contains an image marker or markup only, THEN the system SHALL mark nothing while that
  block is skipped.
- WHEN the book is deleted or reprocessed while playing, THEN the system SHALL stop playback.
- IF the machine has no audio output, THEN the system SHALL report it once and stop, instead of
  marking words against silence.
- WHEN a page is read in a language whose voice is not installed, THEN the system SHALL say which
  voice is missing rather than reading it with the wrong one.
- IF the catalog cannot be consulted because the machine is offline, THEN the system SHALL still
  list and use the voices already installed.
- IF two voices of the same language are installed and the chosen one is deleted from the folder by
  hand, THEN the system SHALL fall back to another installed voice instead of failing.

---

## Requirement Traceability

| Requirement ID | Story | Phase | Status |
| --- | --- | --- | --- |
| TTS-01 | P1: Binário do Piper no instalador, resolvido como os outros componentes | Design | Pending |
| TTS-02 | P1: Síntese por frase, com a duração vinda do áudio gerado | Design | Pending |
| TTS-03 | P1: Palavra corrente marcada, interpolada dentro da frase | Design | Pending |
| TTS-04 | P1: O texto lido é o da página e do idioma na tela | Design | Pending |
| TTS-05 | P1: Tocar, pausar, retomar e parar; ⚠️ **ALTERADO (AD-070)**: virar página ~~para~~ **continua lendo** a página nova se estava tocando; pausado, para; fechar/trocar de livro para | `readAloudStore.ts` — assinatura do `readerStore` + `autoTurn` | Implemented — **sem teste** (sem suíte de frontend); `npm run build` exit 0. Corrige de passagem um defeito lido no código, não reproduzido na tela: a virada manual não parava nada, e a frase antiga, ao terminar, fazia a leitura contínua pular uma página a mais |
| TTS-06 | P1: Uma reprodução por vez | Design | Pending |
| TTS-07 | P1: Nenhuma requisição de rede para ler uma página | Design | Pending |
| TTS-08 | P1: Página `.txt` (PDF e formato antigo) também é lida | Design | Pending |
| TTS-09 | P1: `<script>` removido em qualquer profundidade na extração | Design | Pending |
| TTS-10 | P1: Atributos `on*` removidos na extração | Design | Pending |
| TTS-11 | P1: URLs `javascript:` removidas de `href` e `src` | Design | Pending |
| TTS-12 | P1: iframe com `allow-scripts` e sem `allow-same-origin` | Design | Pending |
| TTS-13 | P1: Componente ausente vira erro nomeado, e o livro segue legível | Design | Pending |
| TTS-14 | P1: O áudio é temporário e nada é escrito na pasta do livro | Design | Pending |
| TTS-15 | P1: Aviso de copyright MIT do Piper acompanhado no bundle | Design | Pending |
| TTS-16 | P1: Avanço automático de página, salvando a posição | Design | Pending |
| TTS-17 | P1: Clicar numa palavra começa a leitura naquela frase | Design | Pending |
| TTS-18 | P1: Clicar durante a reprodução salta para o ponto clicado | Design | Pending |
| TTS-19 | P1: O clique funciona nos dois caminhos de renderização | Design | Pending |
| TTS-20 | P1: Catálogo de vozes, com idioma e tamanho real de download | `tts/voices.rs` — `parse_manifest` | Implemented — **o manifesto do próprio piper**: 176 vozes em 57 idiomas, tamanhos do `size_bytes` dele (conferidos contra `content-length`). 6 embutidas como fallback offline |
| TTS-21 | P1: Baixar uma voz com progresso, sem travar o leitor | `tts_commands.rs`, `VoicesList.tsx` | Implemented — **sem teste**: o download real é UAT |
| TTS-22 | P1: Escolher entre as vozes instaladas de um idioma | `VoicesList.tsx` | Implemented — **sem teste** (sem suíte de frontend) |
| TTS-23 | P1: A voz escolhida por idioma sobrevive ao restart | `config.rs` — `tts_voices` | Implemented — `#[serde(default)]`, config antiga continua carregando |
| TTS-24 | P1: Sem voz para o idioma, o erro nomeia a ação e o livro segue legível | Design | Pending |
| TTS-25 | P1: Idioma lido segue a página, e pode ser fixado pelo usuário | Design | Pending |
| TTS-26 | P1: Download interrompido não deixa voz pela metade | Design | Pending |
| TTS-27 | P1: Testar uma voz instalada sem trocar a voz escolhida | `VoicesList.tsx` — `test()` | Implemented — **sem teste** |
| TTS-28 | P1: A amostra é a primeira frase da página aberta, quando há uma | `VoicesList.tsx` — `test()` | Implemented — **sem teste** |
| TTS-29 | P2: Experimentar um modelo traduzindo um parágrafo da página | - | Pending |
| TTS-30 | P2: O experimento não escreve nada em disco e informa o tempo | - | Pending |
| TTS-31 | P2: Remover uma voz, dizendo o espaço liberado | `tts/voices.rs` — `remove` | Implemented — unit |
| TTS-32 | P1: Velocidade de leitura escolhida e persistida — **também no cabeçalho do leitor** (AD-070) | `tts/speaker.rs` — `length_scale`; `readAloudStore.ts` — `setSpeed`; `ReaderPanel.tsx` | Implemented — unit da inversão e do clamp. O controle do leitor descarta a frase já sintetizada adiante, para a mudança valer já na próxima frase (critério 1); **sem teste de frontend, não ouvido** |
| TTS-38 | P1: Sair da tela do leitor para a leitura em voz alta (AD-070) | `readAloudStore.ts` — assinatura do `uiStore` | Implemented — **sem teste** (sem suíte de frontend); `npm run build` exit 0 |
| TTS-33 | P1: Botão de ouvir no cabeçalho do leitor, com pausar e parar | Design | Pending |
| TTS-34 | P1: Barra de espaço alterna tocar/pausar sem virar página | Design | Pending |
| TTS-35 | P1: Sem voz do Piper instalada, cai no `speechSynthesis` do sistema | Design | Pending |
| TTS-36 | P1: Corpus do sanitizador cobre script aninhado, `onerror`, `onload` e `javascript:` | Design | Pending |
| TTS-37 | P2: Marcação por **palavra**, só depois de a deriva ser medida num livro real | - | Pending |

**Coverage:** 37 total, 0 mapeados para tasks, 37 sem task ⚠️ (a fase de Tasks ainda não rodou)

---

## Success Criteria

- [ ] Um capítulo de um livro real é ouvido do começo ao fim, atravessando páginas sozinho, com a
      marcação acompanhando.
- [ ] Clicar num parágrafo anterior recomeça a leitura dali, com o áudio anterior parando.
- [ ] O deslize da marcação numa frase longa é **medido** e escrito na validação — é o número que
      decide se a interpolação basta ou se o alinhamento forçado volta à mesa.
- [ ] Um EPUB com `<script>` aninhado, `onclick=` e `javascript:` sai do processamento sem nenhum
      dos três, conferido no arquivo em disco.
- [ ] O instalador cresce em uma quantidade **medida**, não estimada, e o número entra no `AGENTS.md`.
      O binário do Piper é o **único** item novo do bundle: nenhuma voz viaja nele. O ponto de
      partida a confirmar é 22.477.236 bytes comprimidos no Windows.
- [ ] O spike da T1 responde, com número, se o binário arquivado aceita várias frases sem
      recarregar o modelo. **Se não aceitar, o desenho do áudio muda** — e é por isso que ele vem
      antes de qualquer código de interface.
- [ ] Duas vozes do mesmo idioma são baixadas do catálogo, testadas na mesma frase do livro, e a
      troca entre elas é ouvida.
- [ ] Com a pasta do Piper renomeada, o app abre, lê e navega o livro normalmente.

---

## O que esta spec muda na `epub-fidelity`

Seguindo `.claude/rules/spec-driven-changes.md` item 4 — a spec antiga **não é apagada**:

- **FID-04 (isolamento: script não roda, CSS não vaza):** **revogado pela metade.** A parte do CSS
  continua valendo integralmente (a origem segue opaca, sem `allow-same-origin`). A parte do script
  muda de mecanismo: o sandbox deixa de ser a defesa e passa a ser a **sanitização na extração**
  (TTS-09, TTS-10, TTS-11). A linha fica marcada na `epub-fidelity` apontando para cá.
  **A defesa passa a exigir manutenção**, que é exatamente o custo que o design da `epub-fidelity`
  tinha recusado — e que o usuário aceitou em 2026-09-07 para ter o karaokê.
- **FID-01/FID-05 (blocos preservados, paginação por bloco):** inalterados. A frase, unidade desta
  feature, é recortada **dentro** do bloco e não muda o que vai para o disco.
- **FID-06/FID-07 (tradução com placeholders):** inalterados. Ler em voz alta lê o que está na tela,
  já traduzido ou não.
- **READ-31 (arquivo por página, legível no explorador):** continua valendo e **ganha**: o `.html`
  em disco passa a ser o texto já sanitizado, então o que se lê no explorador é o que roda na tela.
