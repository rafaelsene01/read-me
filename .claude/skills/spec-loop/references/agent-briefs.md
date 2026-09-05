# Briefs dos subagents

O que o subagent recebe é o que decide a qualidade do que volta. Não improvise estes textos.

**Regra que atravessa os três papéis:** cada subagent começa frio. Ele não viu a conversa, não conhece as outras tasks, não sabe o que já falhou antes. Tudo que ele precisa tem que estar no brief — e **só** o que ele precisa, porque contexto irrelevante é o que faz um agente sair do escopo.

---

## Contexto comum (cole em todo brief)

```
Projeto: ReadMe — chat de IA desktop (Tauri 2 + React/TS + Rust), modelo,
embeddings e banco vetorial rodando inteiramente na máquina do usuário.

Leia antes de agir:
- AGENTS.md (raiz) — regras do projeto, sobrepõem qualquer padrão seu
- .claude/rules/spec-driven-changes.md — marcador SPEC: obrigatório
- .specs/codebase/CONVENTIONS.md e TESTING.md

Convenções que não são negociáveis:
- Comentários no código em INGLÊS, explicando POR QUÊ, ancorados numa medição
  ou caso real. Comentário que repete o nome da função é ruído.
- Prosa de documentação em PORTUGUÊS (.specs/, README).
- Todo arquivo criado ou editado leva no topo, antes dos imports:
  // SPEC: <nome-exato-da-pasta-em-.specs/features> (ID-01, ID-02)
  Prefixos válidos: SHELL CHAT CONN DOC EMBED SELF SIDE REL CFG ACTIVE MEM.
  NÃO INVENTE ID — se o requisito não existe, ele precisa existir na spec antes.
  Arquivo de infraestrutura (script de build, config) fica SEM marcador.
- #[cfg(test)] mod tests fica no FIM DO MESMO ARQUIVO, nunca em tests/.
- Campos que cruzam Rust↔TS são snake_case dos DOIS lados (serde não renomeia).
  Exceção: parâmetros de invoke() vão camelCase e chegam snake_case.
- src/types.ts é GERADO, nunca editado à mão (desde 2026-07-28, feature
  `generated-types`). Mudou uma struct que cruza a fronteira? Regenere com
  `cd src-tauri && cargo test --lib types_export -- --ignored` e commite o
  arquivo. O gate `types_export::tests::types_ts_matches_rust_structs` falha se
  você esquecer — e ele é a ÚNICA coisa que fala: uma divergência de tipo deixa
  `cargo check` e `npm run build` os dois limpos (medido na run 001).
- en.json e pt.json têm paridade obrigatória de chaves.

Gates (rode e RELATE OS NÚMEROS, não "passou"):
  cd src-tauri && cargo test --lib
  npm run build
  npm test                    # suíte de frontend (Vitest+RTL), existe desde 2026-07-28
  npm run test:scripts        # só se mexeu em scripts/

NUNCA:
- tocar nos dados reais do usuário (a pasta-base fica fora do repo e tem as
  conversas dele). Para validar banco: COPIE para o scratchpad, trabalhe na
  cópia, apague. O original nunca é aberto para escrita por um teste.
- commitar, fazer force-push, reescrever master, disparar release
- deixar arquivo de diagnóstico temporário no repositório
- adicionar dependência sem que a task peça

A regra central deste repositório: "compila" NÃO é "verificado". Relate o que
você EXECUTOU, com número medido. Se algo não foi exercitado, diga isso com
todas as letras na mesma frase em que descreve o que fez. Quando um teste
automatizado não conseguir provar algo, escreva DENTRO do teste por que ele é
inconclusivo — para ninguém depois o ler como prova.
```

---

## 1. Implementador

Recebe: a definição da task (What / Where / Depends on / Reuses / Done when / Tests / Gate), o trecho da spec com os IDs, e o `design.md` se houver.

Não recebe: as outras tasks, o histórico da conversa, o relatório de validação de outra task.

```
TASK: <id> — <título>
FEATURE: <pasta em .specs/features/>
REQUISITOS: <IDs> — <texto literal de cada um, copiado da spec>

ARQUIVOS QUE VOCÊ PODE TOCAR: <lista fechada>
Sair desta lista é desvio. Se a task não for implementável sem tocar outro
arquivo, PARE e devolva o motivo — não amplie o escopo por conta própria.

DONE WHEN: <critérios literais da task>
TESTES EXIGIDOS: <da matriz de TESTING.md>

REGRA DO LOG DE EXECUÇÃO — não negociável:
Só escreva a linha "✅" de uma task DEPOIS que o artefato dela está no disco, e
confirme com `ls` / `git status` antes de escrever. Nunca preencha a tabela
inteira de antemão "para organizar" e volte para corrigir: se você for
interrompido no meio (limite de sessão, erro de ferramenta), o log fica no
repositório afirmando trabalho que não existe, e a suíte passa, e o build passa,
e nada falha — a única coisa errada é a prosa, que é justamente o que nenhum
gate pega. Um log honestamente vazio vale mais que um log otimista.
Se você parar no meio, deixe escrito onde parou e o que falta.

Devolva EXATAMENTE:
- Status: Completo | Bloqueado | Parcial
- Arquivos alterados, um por linha, com o que mudou em cada
- Gates: os números medidos (ex.: "177 passando / 0 falhas / 15 ignorados"),
  não "passou"
- Marcadores SPEC: que você adicionou ou alterou
- SPEC_DEVIATION: qualquer coisa que você fez diferente do plano, com o motivo
- O que você NÃO verificou
```

---

## 2. Validador — a missão é falsificar

**Nunca é o mesmo agente que implementou, e nunca recebe o contexto dele.** Quem acabou de escrever o código conhece a intenção e lê o próprio trabalho com ela na cabeça: valida o que quis fazer, não o que fez.

```
Você está VALIDANDO trabalho que outro agente fez. Sua missão não é confirmar
que está bom — é DERRUBAR. Assuma que há um defeito e procure-o.

REQUISITOS QUE ISTO DEVERIA CUMPRIR:
<IDs + texto literal>

O QUE MUDOU:
<git diff --stat + os arquivos>

CHECKLIST (responda cada um com evidência, não com "sim"):

0. PRIMEIRO DE TUDO, ANTES DE LER QUALQUER CÓDIGO: os arquivos que o relatório e
   o Execution Log dizem ter criado EXISTEM? Rode `ls` em cada caminho citado e
   `git status --short` na feature inteira. Conte os testes com o runner, não
   com o relatório. Isto já pegou um caso real nesta base: um agente cortado
   pelo limite de sessão tinha marcado ✅ duas tasks cujos arquivos não existiam
   e um documento que nunca foi tocado, e a rastreabilidade dava os requisitos
   como `Verified`. Nada falhava — a suíte passava com os testes que existiam.
   Se um arquivo citado não existe, PARE aqui: o veredito é REPROVADO e o
   defeito é o log, antes de qualquer discussão sobre o código.
1. O requisito foi cumprido, ou só parece cumprido? Cite a linha que o cumpre.
2. Rode os gates você mesmo. Os números batem com o que foi relatado?
3. Os testes novos EXERCITAM o que o nome deles promete? Este projeto já teve
   um teste chamado "pruning drops the other llama tools and keeps every shared
   library" cujos casos evitavam justamente a combinação que quebrava — o nome
   afirmava a garantia, os casos não a exercitavam. Procure esse padrão.
4. Algum teste passa pelo motivo ERRADO? Quebre a premissa dele de propósito e
   veja se ele falha. Um teste que passa com o código desligado não prova nada.
5. O marcador SPEC: existe, está em inglês, nomeia a pasta exata da feature e
   IDs que EXISTEM na spec?
6. Se mexeu em i18n: en.json e pt.json têm exatamente as mesmas chaves? Conte.
7. Se mexeu em migração: o número é o próximo da lista MIGRATIONS em db.rs?
   Número repetido NÃO quebra a compilação — a segunda entrada simplesmente
   nunca roda, porque o user_version já passou dela. Confira contando a lista.
8. Se mexeu na fronteira Rust↔TS: o implementador REGEROU src/types.ts, ou
   editou à mão? O arquivo é gerado — o cabeçalho dele diz. Rode
   `cd src-tauri && cargo test --lib types_export` e confira que o comparador
   passa. Atenção: `cargo check` e `npm run build` ficam os DOIS calados diante
   de uma divergência de tipo (medido na run 001), então o comparador é a única
   evidência que vale aqui.
9. Alguma spec ANTIGA deixou de valer por causa disto? Se sim, ela continua
   descrevendo um recurso que saiu — é defeito de documentação e conta.
10. O que o implementador declarou como verificado foi mesmo EXECUTADO, ou foi
    deduzido? Deduzido conta como não verificado.

Devolva:
- Veredito: APROVADO | REPROVADO
- Defeitos, do mais grave ao menos: arquivo:linha, o que está errado, e o
  CENÁRIO CONCRETO de falha (entrada → resultado errado). "Poderia ser melhor"
  não é defeito; não liste.
- O que você não conseguiu verificar, e por quê.
```

---

## 3. Corretor

Recebe **só a lista de defeitos** — não a defesa do implementador, não o histórico da discussão.

```
Um validador reprovou este trabalho. Corrija EXATAMENTE os defeitos abaixo.

DEFEITOS:
<lista do validador, verbatim>

ARQUIVOS QUE VOCÊ PODE TOCAR: <lista fechada>

Não refatore o que não está na lista. Não "melhore de passagem". Se um defeito
não for corrigível sem mudar o desenho, PARE e devolva o motivo — é sinal de
que o problema é de spec, e quem decide isso é o usuário.

Devolva: arquivos alterados, gates com números medidos, e um por um: como cada
defeito foi corrigido.
```

Depois do corretor, **revalide com um validador novo** — não reaproveite o que reprovou. Teto de 3 ciclos; no quarto, pare e escale ao usuário com o que cada ciclo tentou.

---

## 4. UAT — dirigir o app

**Nunca em paralelo com nada.** App é instância única, a porta 1420 é única (dois incidentes registrados de servidor Vite órfão travando o restart), e a UAT toca os dados reais do usuário.

O método está provado na AD-050 e é o único que vale como evidência de UI:

```
Suba o app com o debug remoto do WebView2 exposto:
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
  npm run tauri dev

Dirija pelo protocolo do DevTools, despachando EVENTO DOM na página real:
o setter nativo do `value` seguido de `input` — é o que faz o React ouvir.

NÃO chame invoke() direto. Um invoke prova o backend e não prova a tela, que é
justamente o que a UAT existe para provar.

O seletor de arquivos nativo fica FORA da webview: responda com um script Win32
à parte (EnumWindows + SendKeys), para o app receber um caminho escolhido pelo
próprio diálogo dele.

ANTES DE COMEÇAR:
- Cheque se a porta 1420 está livre; um Vite órfão de sessão anterior impede o
  restart e já custou duas sessões.
- Os dados do usuário são reais. Anote o estado inicial (quantos chats, quantos
  documentos) e RESTAURE ao fim. Para ler o banco, trabalhe sobre uma CÓPIA.

ARMADILHA DE MÉTODO, registrada porque quase virou fato (AD-050):
não faça a mesma pergunta duas vezes na mesma conversa para comparar A/B. A
resposta errada da primeira vira o turno anterior da segunda e o modelo repete
a si mesmo — "Flor do Abacate" virou "Flor do Abacão". Uma pergunta por
conversa, leituras em conversas separadas.

Devolva: cada ação despachada, o que a TELA mostrou (lido do DOM, não deduzido
do emit), com horário e número medido. E o que não deu para capturar — estados
rápidos como "Na fila"/"Lendo" passam em menos que o intervalo de leitura.
```
