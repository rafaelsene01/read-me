# Quick Task 008: A poda deixa um lançador órfão, e o AppImage é o único que reclama

**Date:** 2026-07-28
**Status:** Especificado — **não implementado** (o usuário pediu a spec sem execução)
**Feature afetada:** `self-contained-runtime` (SELF-09, SELF-11), `release-distribution` (REL-08)
**Sucede:** a quick 005, cuja hipótese (`strip`) o log desta execução **derruba** — ver abaixo

## Description

O `tauri build --bundles deb,appimage` empacota o `.deb` e **morre no AppImage**:

```
Deploying dependencies for ELF file .../resources/llama/vulkan/llama-b10146/llama
ERROR: Could not find dependency: libllama-cli-impl.so
ERROR: Failed to deploy dependencies for existing files
failed to bundle project: `failed to run linuxdeploy`
```

`shouldPrune` (`scripts/vendor-runtime.mjs`) **mantém o lançador unificado `llama`** — ele não casa
com `llama-`, porque não tem hífen — e **poda as bibliotecas que ele carrega**, todas chamadas
`llama-<subcomando>-impl`. O bundle sai com um binário que não tem como executar, e o `linuxdeploy`,
que resolve as dependências de todo ELF do AppDir, aborta na primeira que falta.

## O que é evidência medida, e o que não é

**Medido, na árvore vendorizada desta máquina** (`node`, varrendo os bytes de cada arquivo à procura
de nomes `*-impl.dll` que não existem no diretório):

| Diretório | Referências pendentes |
| --- | --- |
| `resources/llama/vulkan` | 7, **todas de `llama.exe`**: `llama-cli`, `llama-completion`, `llama-bench`, `llama-batched-bench`, `llama-fit-params`, `llama-quantize`, `llama-perplexity` (sufixo `-impl.dll`) |
| `resources/llama/cpu` | as mesmas 7, também todas de `llama.exe` |

**`llama.exe` é o único arquivo da árvore inteira com referência pendente.** `llama-server.exe`
importa só `llama-server-impl.dll`, que está lá; `ggml-rpc-server.exe` e `llama-common.dll` não têm
nenhuma. Ou seja: o defeito é do Windows também — lá ele não quebra o build porque nada checa
importações na hora de empacotar, e o app nunca chama esse binário. **São 84 KB de lançador morto
publicados desde o M9, nos dois instaladores.**

O mesmo arquivo aparece no log do runner Linux com o nome `llama`, no fim da listagem do AppDir, e
`libllama-cli-impl.so` não aparece em lugar nenhum dela — o que fecha o diagnóstico contra o log, e
não por analogia com o Windows.

**Consequência para os consertos possíveis:** são **7** bibliotecas ausentes, não uma. Trazer de
volta a que o `linuxdeploy` citou faria o build falhar mais seis vezes.

## O que este log corrige na quick 005

A 005 apostou que o `linuxdeploy` abortava ao rodar `strip` em binário de terceiro, e pôs
`NO_STRIP=true` no `release.yml`. **A hipótese estava errada** — ou, no mínimo, era incompleta: com
`NO_STRIP` ativo, o `linuxdeploy` chegou muito mais longe (criou o AppDir, copiou ~100 bibliotecas
do sistema) e falhou em **resolução de dependência**, que é outra etapa.

O que a 005 acertou foi o `--verbose`: ele é a razão de esta falha chegar com o nome do arquivo
ausente anexado, em vez de `failed to run linuxdeploy` e nada mais. Sem ele, este diagnóstico teria
custado outra rodada de correlação entre artefatos publicados.

**Nada a desfazer:** `NO_STRIP` continua barato (o binário do app já sai stripado pelo perfil de
release, REL-27) e continua sendo o que impede o `linuxdeploy` de mexer nos arquivos vendorizados,
que é a lição da AD-046. Ele deixa de ser "o conserto" e passa a ser precaução.

## Approach

Duas mudanças, no mesmo arquivo de script mais o teste.

### 1. `llama` é uma ferramenta, e vai embora

```js
// hoje: `llama` escapa porque a regra pede o hífen
return tool.startsWith("llama-") && tool !== "llama-server";
```

Passa a podar também o `tool` exatamente igual a `llama`. `llama.dll` / `libllama.so` continuam
intocados — bibliotecas nunca chegam nesse ramo (`isLibrary` já as devolve como `null`).

### 2. Uma guarda que falha o build quando sobra referência pendente

Depois de podar cada alvo, o script varre os arquivos **sobreviventes** procurando, byte a byte, o
nome de cada arquivo **removido cuja forma seja de biblioteca** (`*.dll`, `*.so`, `*.so.N`). Se
achar, o `npm run vendor` aborta nomeando os dois lados. Nomes de biblioteca em DT_NEEDED (ELF) e no
diretório de importação (PE) são ASCII cru dentro do arquivo, então a busca não precisa de parser,
de dependência nova, nem de ferramenta externa — o que importa aqui, porque quem builda para Linux é
um runner Ubuntu e quem desenvolve está no Windows.

**Só nomes com forma de biblioteca entram na busca**, e isto foi medido, não suposto: procurar o
nome nu da ferramenta dá falso positivo — a string `llama-cli` existe dentro do próprio `llama.exe`
(é o nome do subcomando). Já `llama-cli-impl.dll` aparece só onde é dependência de verdade.

**Por que não a guarda que eu tinha desenhado.** A ideia inicial era parear lançador e `-impl` pelo
nome e reclamar quando só um dos dois sobrevivesse. **Ela não pegaria este bug:** o lançador se chama
`llama` e a biblioteca `libllama-cli-impl.so` — o par não casa por nome, porque um binário
multiplexador carrega o `-impl` de *outro* subcomando. A guarda tem que olhar a referência real.

## Files Changed

- `scripts/vendor-runtime.mjs` — `shouldPrune` passa a podar `llama`/`llama.exe`; `pruneTree` passa
  a devolver a lista do que removeu, e `main` roda a verificação de referência pendente por alvo
- `scripts/vendor-runtime.test.mjs` — casos novos: `llama`/`llama.exe` podados enquanto
  `llama.dll`/`libllama.so`/`llama-server` sobrevivem; e a verificação de referência, exercitada
  contra uma árvore de arquivos falsos no scratchpad (um "binário" com o nome da lib apagada
  dentro, um sem)

Três arquivos, contando o `TASK.md`. Dentro do teto de quick task.

## Alternativas descartadas, com o motivo

| Alternativa | Por que não |
| --- | --- |
| Manter `llama` e parar de podar as `-impl` que ele usa | São **7** bibliotecas, medidas acima — na prática é desfazer a poda das ferramentas inteira. O app só executa `llama-server` (`runtime/bundled.rs:47-49`, `runtime/process.rs`); nada em Rust jamais chama `llama` |
| Inverter a regra: manter **só** `llama-server` entre os executáveis, podar todo o resto | É a regra mais limpa que existe aqui — acaba com o palpite de nome de uma vez, e `onnxruntime` e `pdfium` não trazem executável nenhum (conferido: só `.dll`/`.so` e documentação), então não haveria dano colateral fora do llama. **Fora de escopo por decisão do usuário nesta sessão:** ela também levaria o `ggml-rpc-server`, que foi decidido deixar como está. Fica registrada como a próxima simplificação, se a guarda voltar a disparar |
| Só o conserto mínimo, sem guarda | É a segunda vez que a mesma classe de defeito passa (a primeira foi a quick 001, na `-impl` do servidor, no Windows). Nas duas o sintoma foi um binário que não carrega, e nas duas a descoberta veio tarde — uma pelo `0xC0000139`, outra por 7 minutos de CI |

## Fora de escopo

`ggml-rpc-server` sobrevive à poda hoje (não começa com `llama-`) e o app nunca o executa. **Não
entra:** o `linuxdeploy` resolveu as dependências dele sem erro (log) e a varredura local não achou
referência pendente nele, então não está quebrado; e o ganho é **101.376 B por variante, ~200 KB no
total** — medido, e pequeno demais para justificar mexer numa poda que já quebrou o runtime duas
vezes (AD-046, quick 001).

## Verification

Gates que rodam nesta máquina:

- [ ] `npm run test:scripts` — precisa subir de 44 (baseline da quick 001) com os casos novos
- [ ] `npm run vendor -- --force` termina sem erro e **imprime o tamanho novo** de cada alvo (o de
      hoje é 113,4 MB vulkan / 61,8 MB cpu no Linux; a queda esperada é ~84 KB por variante)
- [ ] a varredura de referência pendente, rodada de novo à mão sobre `resources/llama/*`, devolve
      **zero** — hoje devolve 7 em cada variante
- [ ] `llama-server.exe --list-devices` do bundle Vulkan continua em exit 0 e listando a GPU (o
      teste que a quick 001 estabeleceu; a poda não pode regredir de novo)
- [ ] `npm run build` e `cd src-tauri && cargo test --lib` (177 passando) — nenhum dos dois toca
      este script, e é exatamente por isso que precisam ser rodados: para provar que não toca

O que **não** dá para verificar aqui:

- [ ] ⛔ **O AppImage.** Isto é Linux, o `linuxdeploy` não roda no Windows, e a única prova é um
      `workflow_dispatch` do `release.yml` que chegue a publicar o `.AppImage`. **Rodar a varredura
      e ver zero não é o AppImage empacotado** — é a L-005 deste repositório, pela terceira vez
- [ ] ⛔ O `.deb` publicado hoje carrega o mesmo lançador morto. Ele **empacota** (o `dpkg` não olha
      importação), então o conserto não muda o resultado do build dele — só o conteúdo

## Rastreabilidade a atualizar quando isto for executado

- `self-contained-runtime/spec.md`: SELF-09 e SELF-11 continuam **sem nenhuma medição no Linux** —
  esta mudança não altera isso, e a linha não pode passar a sugerir que altera
- `release-distribution/spec.md`: REL-08 está **Verified** com base na v0.2.0, que é anterior ao M9.
  A observação de que o AppImage nunca foi empacotado com a árvore vendorizada precisa entrar lá
- `.specs/project/STATE.md`: AD-051 registrando a guarda de referência e o motivo de ela não ser
  por pareamento de nome; e a linha da quick 005 corrigida — a hipótese do `strip` caiu

## Commit

Não commitado, e nem implementado. Mensagem prevista quando for:
`fix(vendor): prune the orphaned llama launcher and fail on dangling deps`
