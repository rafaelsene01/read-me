# Cobertura de teste no frontend — Specification

**Origem:** C-04 de `.specs/codebase/CONCERNS.md`
**Milestone:** dívida técnica (fora da numeração de M)

## Problem Statement

O frontend não tem runner de teste. `package.json` não declara Vitest, Jest, Testing Library nem
script `test`, e `.specs/codebase/TESTING.md` registra "none (por ora)" para componentes React.

O backend tem 177 testes; o frontend tem zero. E o frontend cresceu: **19 componentes e 6 stores**
(contados em 2026-07-27) contra os 12 e 4 de quando o C-04 foi escrito — **58% a mais de superfície,
com a cobertura parada no mesmo lugar**.

A lógica que o C-04 nomeia não é decorativa. Três exemplos do que hoje ninguém prova:

- o listener de `memory-backfill-progress` **descarta eventos de outra conversa** (`chatStore.ts`).
  Se essa guarda cair, a barra de progresso de um chat passa a ser dirigida pelo backfill de outro;
- o listener de `model-download-progress` indexa por **URL do `.gguf`** (`runtimeStore.ts`). Trocar
  a chave por qualquer outra coisa faz a barra do modelo A mostrar o download do modelo B;
- o filtro `fits_ram` (`ModelsList.tsx`) decide o que o usuário vê ao abrir a tela de modelos.
  Invertê-lo mostra só o que **não** cabe na RAM da máquina.

Nenhum desses defeitos quebra o `tsc`. Todos aparecem só em runtime, na frente do usuário — o mesmo
padrão que o `AGENTS.md` já registra para o `invoke` por string.

## Goals

- [x] Um comando (`npm test`) que roda a suíte de frontend e **reprova** quando a lógica quebra
- [x] A lógica nomeada pelo C-04 exercitada de verdade, não "o store existe"
- [x] Cada teste validado por **mutação**: quebrar a lógica de propósito e ver o teste falhar
- [x] `.specs/codebase/TESTING.md` deixando de dizer que não há runner

## Out of Scope

| Item | Motivo |
| --- | --- |
| Cobertura de 100% dos 19 componentes | O C-04 recomenda começar pelos stores — lógica pura, maior risco por menor esforço. Cobrir toda a árvore de JSX é ceremônia com pouco retorno |
| Teste de integração com o backend Rust de pé | O `TESTING.md` já registra que não há runner de integração Tauri; um teste de frontend que sobe o app não é teste de unidade, é UAT |
| Snapshot testing de JSX | Snapshot passa a quebrar em toda mudança de classe Tailwind e ensina a aceitar o diff sem ler — o oposto do que este item quer |
| Cobertura mínima obrigatória no CI (threshold) | Um número de cobertura convida a escrever teste para o número. O gate aqui é a mutação, não o percentual |
| Ligar `npm test` no `ci.yml` | `.github/workflows/` está fora dos arquivos que esta task pode tocar. Fica registrado como pendência |
| Testar `src/types.ts` | Está sendo regenerado pela C-03; asserção sobre a forma literal dele nasceria morta |

---

## User Stories

### P1: Existe um runner e ele reprova ⭐ MVP

**User Story**: Como agente ou desenvolvedor trabalhando neste repositório, quero um comando que
rode os testes de frontend, para que uma quebra de lógica apareça antes de chegar ao usuário.

**Why P1**: Sem runner, nada mais desta spec pode existir.

**Acceptance Criteria**:

1. WHEN `npm test` é executado THEN o sistema SHALL rodar a suíte de frontend uma única vez (sem
   modo watch) e sair com código diferente de zero se algum teste falhar
2. WHEN um teste importa um store THEN o ambiente SHALL oferecer `window`, `document` e
   `localStorage` — `src/i18n/index.ts` lê `localStorage` no import, e `src/lib/theme.ts` escreve
   em `document.documentElement`
3. WHEN um teste importa qualquer módulo que fale com o Tauri THEN nenhuma chamada IPC real SHALL
   sair: `@tauri-apps/api/core` e `@tauri-apps/api/event` são substituídos por dublês
4. WHEN a suíte de frontend é adicionada THEN `npm run build` e `npm run test:scripts` SHALL
   continuar produzindo o mesmo resultado de antes, e nenhum arquivo de teste SHALL entrar no bundle

**Independent Test**: rodar `npm test` num clone limpo, com o backend parado, e ver a contagem.

---

### P1: Os listeners de evento são exercitados ⭐ MVP

**User Story**: Como usuário com duas conversas abertas, quero que o progresso, o streaming e os
avisos de uma não apareçam na outra.

**Why P1**: É o defeito que o C-04 nomeia primeiro, e é invisível ao `tsc`. Os listeners são
registrados no **import do módulo**, fora do store — não há como chegar neles por uma ação de UI.

**Acceptance Criteria**:

1. WHEN chega um `memory-backfill-progress` de um chat diferente do que está indexando THEN o
   estado `memoryIndexing` SHALL permanecer intocado
2. WHEN chega um `memory-backfill-progress` e nenhuma indexação está rodando THEN nada SHALL ser
   escrito no estado
3. WHEN chega um `chat-stream-chunk` de um chat que não é o `streamingChatId` THEN o delta SHALL ser
   descartado, mesmo que aquele chat seja o que está na tela
4. WHEN chegam vários `chat-stream-chunk` do chat em streaming THEN os deltas SHALL ser concatenados
   na ordem de chegada
5. WHEN um `chat-stream-chunk` traz `error` de um chat que **não** é o ativo THEN a geração SHALL
   terminar sem mostrar o erro na tela do chat que o usuário está lendo
6. WHEN chega um `chat-retrieval-warning` de um chat que não é o ativo THEN nenhum aviso SHALL ser
   exibido
7. WHEN chega um `model-download-progress` THEN o progresso SHALL ser guardado sob o `identifier` do
   evento e os progressos dos outros downloads SHALL permanecer intactos
8. WHEN chega um `document-status` THEN apenas o documento cujo `id` bate SHALL ser alterado

**Independent Test**: disparar o handler registrado com um payload de outra conversa e comparar o
estado antes e depois.

---

### P1: As ações assíncronas dos stores respeitam o chat que o usuário está lendo ⭐ MVP

**User Story**: Como usuário, quero poder trocar de conversa enquanto uma resposta é gerada, sem que
a conversa que abri seja substituída pela que ficou para trás.

**Why P1**: `sendMessage` faz cinco checagens de escopo no `finally`. São exatamente o tipo de
condição que some numa refatoração e não quebra compilação nenhuma.

**Acceptance Criteria**:

1. WHEN `sendMessage` é chamado sem chat ativo THEN nada SHALL acontecer e nenhum comando SHALL ser
   invocado
2. WHEN `sendMessage` é chamado THEN a mensagem do usuário SHALL aparecer imediatamente na lista,
   antes de o backend responder
3. WHEN a geração termina e o usuário já trocou de conversa THEN as mensagens recarregadas SHALL ser
   descartadas, e a lista exibida SHALL continuar sendo a da conversa aberta
4. WHEN `sendMessage` falha para um chat que não é mais o ativo THEN o erro SHALL **não** ser exibido
5. WHEN um toggle otimista (`setUseGlobalRag`, `setUseMemory`) falha no backend THEN a lista de chats
   SHALL voltar exatamente ao estado anterior
6. WHEN um chat que **não** é o ativo é excluído THEN a conversa aberta SHALL permanecer na tela
7. WHEN qualquer comando de store rejeita THEN o erro SHALL virar string em `error` e a promise do
   store SHALL resolver — nunca propagar exception para o componente

---

### P2: Config e update têm suas decisões cobertas

**User Story**: Como usuário que renomeou um tema há duas versões e cujo HD externo não está
montado, quero que o app faça a coisa certa nos dois casos.

**Why P2**: Lógica menos exposta que a dos listeners, mas com o mesmo formato: decisão em `if`, sem
teste.

**Acceptance Criteria**:

1. WHEN o tema salvo tem um id renomeado THEN `normalizeTheme` SHALL devolver o id novo, e o tema
   SHALL ser reescrito no disco **uma única vez**
2. WHEN o tema salvo já é válido THEN nenhuma reescrita SHALL acontecer
3. WHEN a pasta de armazenamento configurada não está pronta THEN o app SHALL ir para o onboarding
   carregando o caminho que faltou
4. WHEN a checagem de update no boot roda com `auto_check` desligado THEN nenhuma checagem SHALL
   sair
5. WHEN a checagem de update no boot falha THEN nenhum erro SHALL ser escrito no estado — estar
   offline é o normal deste app

---

### P2: A lógica de apresentação nomeada pelo C-04 é coberta

**User Story**: Como usuário abrindo a tela de modelos, quero ver primeiro o que roda na minha
máquina, e uma barra de progresso que corresponde ao download.

**Why P2**: Vive dentro de componente (`useMemo` e expressão inline), então custa mais que um store
— mas é citada nominalmente pelo C-04.

**Acceptance Criteria**:

1. WHEN a lista de modelos baixáveis é renderizada THEN apenas os modelos com `fits_ram` verdadeiro
   SHALL aparecer
2. WHEN o toggle "mostrar todos" é marcado THEN os modelos que não cabem SHALL aparecer também
3. WHEN há progresso com `downloaded_bytes` e `total_bytes` THEN o percentual exibido SHALL ser o
   arredondado da razão, limitado a 100
4. WHEN o progresso não traz `total_bytes` THEN a barra SHALL ficar em 0% em vez de exibir `NaN`

---

### P1: Cada teste é provado por mutação ⭐ MVP

**User Story**: Como quem vai ler este trabalho depois, quero saber que os testes falham quando o
código quebra, e não apenas que eles passam.

**Why P1**: A AD-046 registra um teste desta base cujo nome afirmava uma garantia que os casos
escolhidos não exercitavam — o defeito passou e quebrou o app na primeira execução real. Um teste
que passa com a lógica desligada é pior que teste nenhum: ele dá cobertura falsa.

**Acceptance Criteria**:

1. WHEN um teste desta spec é escrito THEN a lógica que ele cobre SHALL ser quebrada de propósito e
   o teste SHALL ser observado falhando, com a mutação e a falha registradas
2. WHEN um teste não consegue provar o que o nome dele afirma THEN a limitação SHALL ser escrita
   **dentro do arquivo de teste**, para ninguém depois o ler como prova

---

## Requirements Traceability

| ID | Requisito |
| --- | --- |
| FTEST-01 | `npm test` roda a suíte uma vez e reprova com código de saída |
| FTEST-02 | Ambiente com DOM e `localStorage` (o import de `i18n` depende disso) |
| FTEST-03 | Nenhuma chamada IPC real: `@tauri-apps/api` dublado |
| FTEST-04 | `npm run build` e `test:scripts` intactos; teste fora do bundle |
| FTEST-05 | `memory-backfill-progress` de outro chat é descartado |
| FTEST-06 | `memory-backfill-progress` sem indexação rodando é descartado |
| FTEST-07 | `chat-stream-chunk` de chat que não está em streaming é descartado |
| FTEST-08 | Deltas do chat em streaming são concatenados na ordem |
| FTEST-09 | `error` de chat não-ativo encerra a geração sem aparecer na tela |
| FTEST-10 | `chat-retrieval-warning` só para o chat ativo |
| FTEST-11 | `model-download-progress` indexado por `identifier`, sem perder os outros |
| FTEST-12 | `document-status` altera só a linha do `id` do evento |
| FTEST-13 | `sendMessage` sem chat ativo não invoca nada |
| FTEST-14 | A mensagem do usuário aparece antes de o backend responder |
| FTEST-15 | Troca de conversa durante a geração não substitui a conversa aberta |
| FTEST-16 | Erro de `sendMessage` de chat não-ativo não é exibido |
| FTEST-17 | Toggle otimista reverte a lista no erro |
| FTEST-18 | Excluir chat não-ativo preserva a conversa aberta |
| FTEST-19 | Erro de comando vira string em `error`, sem exception |
| FTEST-20 | Tema renomeado é normalizado e reescrito no disco uma vez |
| FTEST-21 | Pasta de armazenamento ausente manda ao onboarding com o caminho |
| FTEST-22 | Checagem de boot respeita `auto_check` e é silenciosa no erro |
| FTEST-23 | Filtro `fits_ram` + toggle "mostrar todos" |
| FTEST-24 | Percentual de download arredondado, limitado a 100, sem `NaN` |
| FTEST-25 | Cada teste validado por mutação, com a mutação registrada |
| FTEST-26 | Teste inconclusivo declara a limitação dentro do próprio arquivo |

**Status values:** Pending → In Design → In Tasks → Implementing → Verified

---

## Success Criteria

- [x] `npm test` roda com o backend parado e sem rede, e a contagem é registrada como número —
      **8 arquivos, 63 testes, 3,08 s** (2026-07-28)
- [x] Para cada requisito de FTEST-05 a FTEST-24, existe uma mutação registrada que fez o teste
      correspondente **falhar** — 12 mutações na tabela do `tasks.md`
- [ ] `cargo test --lib` continua em 177/0/15 e `npm run test:scripts` em 49 — **metade
      verificada:** `npm run test:scripts` foi remedido em **49 passando, 0 falhas, 115 ms**
      (2026-07-28). O `cargo test --lib` **não foi rodado** nesta sessão: `src-tauri/` estava
      sendo alterado por outras tasks em paralelo, e o número que sairia mediria o trabalho
      delas, não a ausência de impacto desta spec — que não toca um arquivo Rust sequer
- [x] `.specs/codebase/TESTING.md` descreve o que existe, e não "sem runner"
