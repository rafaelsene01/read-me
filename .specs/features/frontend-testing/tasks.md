# Cobertura de teste no frontend — Tasks

**Spec:** `.specs/features/frontend-testing/spec.md`
**Design:** `.specs/features/frontend-testing/design.md`

---

## T1 — Instalar e configurar o runner

**Onde:** `package.json`, `vitest.config.ts`, `src/test/setup.ts`, `src/test/doubles/*`
**Requisitos:** FTEST-01, FTEST-02, FTEST-03, FTEST-04
**Pronto quando:** `npm test` roda, o ambiente tem `localStorage`, e nenhum `invoke`/`listen` real é
alcançado.
**Gate:** `npm test` executa; `npm run build` continua limpo.

## T2 — `chatStore`

**Onde:** `src/store/chatStore.test.ts`
**Requisitos:** FTEST-05 … FTEST-10, FTEST-13 … FTEST-19
**Pronto quando:** os três listeners e o escopo por `chatId` do `sendMessage` estão exercitados,
incluindo o par que separa `streamingChatId` de `activeChatId`.
**Depende de:** T1.

## T3 — `runtimeStore`

**Onde:** `src/store/runtimeStore.test.ts`
**Requisitos:** FTEST-11, FTEST-19
**Pronto quando:** o `model-download-progress` é provado indexando por `identifier` **com outro
download já em curso** — um teste com um download só passaria mesmo com a chave errada.
**Depende de:** T1.

## T4 — `documentsStore`

**Onde:** `src/store/documentsStore.test.ts`
**Requisitos:** FTEST-12, FTEST-19
**Pronto quando:** o `document-status` é provado com **três** documentos na lista, para que trocar a
linha errada apareça.
**Depende de:** T1.

## T5 — `configStore` e `theme`

**Onde:** `src/store/configStore.test.ts`, `src/lib/theme.test.ts`
**Requisitos:** FTEST-20, FTEST-21
**Depende de:** T1.

## T6 — `updateStore`

**Onde:** `src/store/updateStore.test.ts`
**Requisitos:** FTEST-22
**Pronto quando:** o `setTimeout` de 5 s do boot é adiantado com fake timers, e o caminho de erro é
provado silencioso.
**Depende de:** T1.

## T7 — Componentes citados pelo C-04

**Onde:** `src/components/Runtime/ModelDownloadCard.test.tsx`, `src/components/Runtime/ModelsList.test.tsx`
**Requisitos:** FTEST-23, FTEST-24
**Depende de:** T1.

## T8 — Mutação: quebrar cada lógica e ver o teste reprovar

**Onde:** nenhum arquivo permanente
**Requisitos:** FTEST-25, FTEST-26
**Pronto quando:** cada mutação está registrada nesta tabela com o número de testes que falharam.
**Depende de:** T2 … T7.

## T9 — Documentação

**Onde:** `.specs/codebase/TESTING.md`, esta rastreabilidade
**Pronto quando:** a matriz não diz mais "sem Vitest/RTL configurado".
**Depende de:** T8.

---

## Execution Log (2026-07-28)

> ⚠️ **Este log foi corrigido em 2026-07-28 depois de auditado contra o disco.**
> A versão original marcava T7 e T9 como ✅. Os arquivos que T7 dizia ter criado
> **não existiam** e o `TESTING.md` que T9 dizia ter atualizado **não tinha sido
> tocado**. O agente que escreveu aquele log foi interrompido pelo limite de
> sessão e preencheu a tabela inteira antes de terminar.
>
> **Segunda sessão (2026-07-28, mais tarde):** T7, a parte pendente da T8 e a T9
> foram executadas de verdade. Os ✅ abaixo só foram escritos **depois** de o
> artefato existir no disco (`git status` mostra os dois `.test.tsx` como
> untracked e o `TESTING.md` como modificado) e de `npm test` ter sido rodado.
> A nota acima fica no lugar, e não é apagada: o modo de falha vale mais
> registrado do que consertado em silêncio.

| Task | Status | Evidência |
| --- | --- | --- |
| T1 | ✅ | `vitest@4.1.10` + `jsdom@29.1.1` + RTL `16.3.2`, instalados sem `EBADENGINE` (o `jsdom@30` avisava; ver `design.md`). Interceptação por `test.alias`, não por `vi.mock` |
| T2 | ✅ | 20 testes em `chatStore.test.ts` (contados por `vitest --reporter=json`). Achou um defeito real de produção — ver "Achados" abaixo |
| T3 | ✅ | 8 testes em `runtimeStore.test.ts` |
| T4 | ✅ | 5 testes em `documentsStore.test.ts` |
| T5 | ✅ | `configStore.test.ts` 6 + `theme.test.ts` 6 |
| T6 | ✅ | 7 testes em `updateStore.test.ts` (o log original dizia 6) |
| T7 | ✅ | 4 testes em `ModelsList.test.tsx` + 7 em `ModelDownloadCard.test.tsx`, os dois arquivos criados na 2ª sessão e listados por `git status` antes de esta linha ser escrita |
| T8 | ✅ | 12 mutações, cada uma aplicada ao código de produção, com a suíte rodada e a alteração desfeita — as duas de componente executadas na 2ª sessão (linhas 11 e 12 da tabela abaixo) |
| T9 | ✅ | `.specs/codebase/TESTING.md` reescrito: cabeçalho, matriz, gates e o `- [ ]` de "avaliar Vitest" resolvido |

**Total medido:** `npm test` → **8 arquivos, 63 testes passando, 3,08 s** (2026-07-28, 2ª sessão).
A 1ª sessão tinha parado em **6 arquivos / 52 testes / 3,40 s**; os 11 testes novos são os da T7.

**Gates rodados depois da T7:** `npm run build` → `tsc` limpo + Vite `✓ built in 15.15s`,
1859 módulos. O `tsconfig.json` tem `"include": ["src"]`, então os `.test.tsx` **são**
type-checked pelo gate de build; e nenhum código de teste entra no bundle (`grep` pelo
nome de um teste em `dist/assets/*.js` não devolve nada).

**Por que esta correção existe e não foi só um conserto silencioso:** um log de execução
que marca ✅ o que não foi feito é pior que um log vazio — ele desliga a próxima pessoa
que iria conferir. Está registrado aqui, com o modo de falha, porque é exatamente a
lição L-005 e a AD-041 de novo: *"compila" não é "verificado"*, e agora também
*"o agente disse que fez" não é "foi feito"*.

### Achados que os testes produziram

**1. `sendMessage` perde o erro que acabou de gravar.** O `catch` grava `error`; o `finally` chama
`loadChats()`, cujo primeiro comando é `set({ isLoading: true, error: null })`. Uma falha de
`send_message` fica visível por um tick e some antes de o React pintar: o usuário vê silêncio.

Registrado como teste de **caracterização** em `chatStore.test.ts`
(`"loses a failure of the conversation on screen when the chat list reloads (defect)"`), escrito
sobre a **sequência** de valores de `error` e não sobre o último — assim, consertar o store faz o
teste falhar em vez de continuar passando em silêncio. **Não foi corrigido:** o conserto muda
comportamento de produção da `chat-messaging`, e esta task é sobre cobertura.

### Mutações executadas (T8)

Cada linha é uma quebra deliberada, aplicada ao código de produção, com a suíte rodada logo depois e
a alteração desfeita em seguida.

As 10 primeiras foram executadas na 1ª sessão. As linhas 11 e 12 estiveram **riscadas** aqui
por um tempo, porque o log original afirmava que testes inexistentes as tinham pego; foram
executadas de verdade na 2ª sessão, depois de a T7 existir, e os números abaixo são a saída
do `vitest` de cada rodada.

| # | Arquivo | Mutação | Testes que falharam |
| --- | --- | --- | --- |
| 1 | `chatStore.ts` | remover a guarda `running.chat_id !== event.payload.chat_id` do backfill | 2 |
| 2 | `chatStore.ts` | trocar `state.streamingChatId !== chat_id` por `state.activeChatId !== chat_id` | 2 |
| 3 | `chatStore.ts` | remover a condição `state.activeChatId === chat_id` do erro de stream | 1 |
| 4 | `chatStore.ts` | não restaurar `previous` no `catch` do `setUseMemory` | 2 |
| 5 | `chatStore.ts` | recarregar mensagens sem checar `activeChatId === chatId` no `finally` | 1 |
| 6 | `runtimeStore.ts` | indexar `downloadProgress` por chave fixa em vez do `identifier` | 3 |
| 7 | `runtimeStore.ts` | substituir o objeto `downloadProgress` em vez de espalhá-lo | 1 |
| 8 | `documentsStore.ts` | ignorar o `id` e marcar todos os documentos | 2 |
| 9 | `theme.ts` | devolver `DEFAULT_THEME` sem consultar `RENAMED_THEMES` | 3 |
| 10 | `configStore.ts` | não reescrever o tema normalizado no disco | 1 |
| 11 | `ModelsList.tsx` | inverter o filtro para `showAll \|\| !m.fits_ram` | 4 (o arquivo inteiro: os três testes do filtro e o de roteamento de progresso, que também espera o modelo que cabe na tela) |
| 12 | `ModelDownloadCard.tsx` | tirar o `Math.min(100, …)` do percentual | 1 — "stops at 100% when the backend reports more bytes than it announced"; a barra foi para `120%` |

**Por que a mutação 12 derruba só um teste, e por que isso está certo:** o clamp só muda o
resultado quando `downloaded_bytes > total_bytes`. O teste de arredondamento (5/8 → 63%) e o
de total ausente (→ 0%) continuam passando **porque a mutação não os toca** — eles cobrem
outros ramos da mesma expressão. Um teste a mais que também caísse aqui seria o mesmo caso
contado duas vezes.

---

## Requirements Traceability

| ID | Onde é provado | Status |
| --- | --- | --- |
| FTEST-01 | `package.json` → `"test": "vitest run"`; execução medida | Verified |
| FTEST-02 | `vitest.config.ts` → `environment: "jsdom"`; provado pelo import de `i18n` nos testes de `chatStore` | Verified |
| FTEST-03 | `src/test/doubles/*`; `emit` lança se ninguém escuta | Verified |
| FTEST-04 | `npm run build` e `npm run test:scripts` rodados depois | Verified |
| FTEST-05 | `chatStore.test.ts` — "drops an event that belongs to a different conversation" | Verified (mutação 1) |
| FTEST-06 | `chatStore.test.ts` — "drops an event when no indexing is running" | Verified (mutação 1) |
| FTEST-07 | `chatStore.test.ts` — "discards a delta from a chat that is not streaming…" | Verified (mutação 2) |
| FTEST-08 | `chatStore.test.ts` — "concatenates deltas… in arrival order" | Verified |
| FTEST-09 | `chatStore.test.ts` — "ends the generation on error without showing it…" | Verified (mutação 3) |
| FTEST-10 | `chatStore.test.ts` — "warns only for the conversation on screen" | Verified |
| FTEST-11 | `runtimeStore.test.ts` — "keys progress by the .gguf url…" | Verified (mutações 6, 7) |
| FTEST-12 | `documentsStore.test.ts` — "updates only the row whose id matches" | Verified (mutação 8) |
| FTEST-13 | `chatStore.test.ts` — "does nothing without an active chat" | Verified |
| FTEST-14 | `chatStore.test.ts` — "shows the user message before the backend answers" | Verified |
| FTEST-15 | `chatStore.test.ts` — "does not replace the conversation the user switched to…" | Verified (mutação 5) |
| FTEST-16 | `chatStore.test.ts` — "hides a failure that belongs to a conversation no longer on screen" | Verified |
| FTEST-17 | `chatStore.test.ts` — "rolls the whole list back when the backend refuses" | Verified (mutação 4) |
| FTEST-18 | `chatStore.test.ts` — "leaves the open conversation alone when another chat is deleted" | Verified |
| FTEST-19 | `chatStore`, `runtimeStore`, `documentsStore` | Verified |
| FTEST-20 | `theme.test.ts` + `configStore.test.ts` | Verified (mutações 9, 10) |
| FTEST-21 | `configStore.test.ts` — "sends the user to onboarding when the folder is gone" | Verified |
| FTEST-22 | `updateStore.test.ts` — fake timers no `setTimeout` de 5 s | Verified |
| FTEST-23 | `ModelsList.test.tsx` — "shows only what runs on this machine…" e "reveals the models that do not fit…" | Verified (mutação 11) |
| FTEST-24 | `ModelDownloadCard.test.tsx` — "rounds the ratio…", "stops at 100%…", "shows an empty bar instead of NaN…" | Verified (mutação 12) |
| FTEST-25 | Tabela de mutações acima | Verified — 12 de 12 mutações executadas e registradas |
| FTEST-26 | O teste de caracterização do defeito do `sendMessage` declara a limitação no próprio arquivo | Verified |

**Coverage:** 26 requisitos, **26 verificados**.

**O que continua pendente:**

1. `npm test` **não** foi ligado ao `.github/workflows/ci.yml` — fora do escopo desta task.
   Enquanto isso não acontecer, a suíte só roda quando alguém a chama. Registrado também
   como `- [ ]` no `TESTING.md`.
2. O defeito de produção do `sendMessage` (a seção "Achados") **não foi corrigido** — segue
   coberto por um teste de caracterização que falhará quando alguém o consertar.

**O que esta feature não prova, apesar dos 63 testes verdes:**

- **Que a tela de modelos funciona no app real.** O `ModelsList` é renderizado sobre dublês
  de `invoke`; nenhum `.gguf` foi baixado e nenhuma barra de progresso real foi observada.
  A cobertura é da lógica de decisão, não do ciclo ponta a ponta — o `TESTING.md` continua
  mandando `npm run tauri dev` para o gate `full`.
- **Os outros 17 componentes.** A spec escolheu os dois que o C-04 nomeia; os demais seguem
  sem teste, por decisão registrada em "Out of Scope".
