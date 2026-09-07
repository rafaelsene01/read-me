# T1 — saída bruta da medição de tradução

Gerado em 2026-09-06 pela T1 da feature `book-reader`. A página de entrada é `t1-page-en.txt` (Moby-Dick, cap. 1, Project Gutenberg #2701, 2.161 caracteres em 3 parágrafos). Uma requisição por parágrafo, prompt do design, `llama-server` Vulkan com `-ngl -1`.

Este arquivo existe para que a avaliação de qualidade abaixo seja **auditável**: qualquer pessoa pode ler as três traduções da mesma página e discordar do veredito.

---

## Qwen2.5 1.5B Instruct Q4_K_M

**Medido:** 6.03 s de parede na página inteira, 162.9 tokens/s de geração, 640 tokens de saída, custo fixo 699 ms por requisição (80 ms descontando a primeira, que carrega o aquecimento). Carga do modelo: 9.2 s.

### Parágrafo 1 (1107 caracteres de entrada, 319 tokens de saída, 3914 ms de parede, TTFT 1930 ms)

> Respondo a chamada de Ishmael. Alguns anos atrás —não me lembro exatamente quanto tempo passou—, com pouca ou nenhuma moeda em meu bolso, e nada de particular para me interessar no mar, decidi ir ao mar para ver o seu lado marítimo. É uma maneira que tenho de despejar a meia boca e regular o fluxo sanguíneo. Quando eu me sinto crescer triste sobre o meu rosto; quando é um outono úmido e chuvoso no meu espírito; quando eu me indorem involuntariamente antes das casas de cremes, e me virarmos atrás de cada funeral que encontro; e, especialmente, quando meu hipócrita me domina, que exigirá de mim uma forte vontade moral para evitar deliberadamente entrar na rua, e metacriticamente derrubar o chapéu de alguém —então, considero que é hora de embarcar como o mais cedo possa. Isso é minha substituição para o rifle e balas. Com um encanto filosófico Cato se joga sobre o seu fuzil; eu quietamente me joguei em direção à minha nave. Não há nada extraordinário nisso. Se eles apenas soubessem, quase todos os homens em seu grau, um dia ou outro, carregam sentimentos bastante semelhantes ao mar comigo.

### Parágrafo 2 (387 caracteres de entrada, 128 tokens de saída, 858 ms de parede, TTFT 79 ms)

> Agora, você tem sua ilha do Manhattan, cercada por píer como ilhas indígenas por corais - a comércio acomoda-a com sua onda. À direita e à esquerda, as ruas te levam para dentro do mar. O extremo da sua cidade-downtown é o batalhão, onde aquele mole nobre está sob o influência das ondas, e que foi resfriado por ventos, que anteriormente estavam fora de visão da terra. Olhe para as multidões de observadores de água.

### Parágrafo 3 (663 caracteres de entrada, 193 tokens de saída, 1253 ms de parede, TTFT 67 ms)

> Corleos Hook a Sábado tranquilo, vá de Corleos Hook a Coenties Slip e de lá, seguindo Whitehall, norte. O que veres? - Postados como silenciosos sentinelas por toda a cidade, estão centenas de homens mortais fixos em reverências marinas. Alguns se inclinam contra as espicas; alguns se sentam nos cabeços de ponte; alguns olham para os balões dos navios de China; alguns se elevam em alta voz nas rigas, como se se esforçassem para obter um melhor vislumbre ao leste. Mas esses são todos os marinheiros; dos dias de semana encarcerados em madeira e argamassa - presos às mesas, amarrados à pia, clinhados à mesa. Então, onde estão os campos verdes? O que eles aqui?

---

## Llama 3.2 3B Instruct Q4_K_M

**Medido:** 6.46 s de parede na página inteira, 110.4 tokens/s de geração, 661 tokens de saída, custo fixo 158 ms por requisição (156 ms descontando a primeira, que carrega o aquecimento). Carga do modelo: 6.1 s.

### Parágrafo 1 (1107 caracteres de entrada, 314 tokens de saída, 3030 ms de parede, TTFT 153 ms)

> Chame-me Miqueias. Alguns anos atrás—nem me importa quanto tempo exatamente—ter um pouquinho ou nenhuma dinheiro na minha bolsa, e nada em particular que me interessasse na costa, pensei em navegar um pouco e ver o lado aquático do mundo. É uma maneira que tenho de despejar o estômago e regular o fluxo sanguíneo. Quando me sinto pesado no boca; quando é um novembro úmido e chuvoso na minha alma; quando me sinto pausando involuntariamente em frente às lojas de caixões, e trazendo as costas atrás de todos os funerais que encontro; e especialmente quando meus depressões têm um controle tão forte sobre mim, que requer uma força moral forte para impedir-me de deliberadamente bater em estrada e methodicamente arrancar os chapéus das pessoas—então, considero que é hora de sair do mar o mais rápido possível. É a minha substituta pelo pistol e bala. Com um movimento filosófico Cato lança-se contra a sua espada; eu tomo o barco em silêncio. Não há nada surpreendente nisso. Se eles apenas soubessem, quase todos os homens em algum grau, alguma vez, sentirão quase o mesmo sentimento em relação ao oceano comigo.

### Parágrafo 2 (387 caracteres de entrada, 123 tokens de saída, 1270 ms de parede, TTFT 163 ms)

> Agora está sua cidade insular dos Manhattoes, cercada por docas como ilhas indianas são cercadas por rifa de coral—o comércio a cerca dela com seus surtos. À direita e à esquerda, as ruas te levam para o mar. Seu extremo centro é a bateria, onde aquela nobre ponta é banhada por ondas e resfria-se com ventos, que alguns horas antes estavam fora de vista da terra. Olhe para as multidões de observadores do mar ali.

### Parágrafo 3 (663 caracteres de entrada, 224 tokens de saída, 2163 ms de parede, TTFT 130 ms)

> Circunnavegue a cidade de um apósno de fim de semana sonhador. Vá de Corlears Hook para Coenties Slip, e daí, por Whitehall, em direção ao norte. O que você vê?—Postados como sentinelas silenciosas em volta da cidade, estão milhares e milhares de homens mortais fixados em reverências ao mar. Alguns se apoiam contra os esguilhas; outros sentados sobre os cabeceiras dos cais; outros olhando para as muralhas dos barcos de China; outros a bordo, a uma altura notável, como se estivessem tentando obter uma vista ainda melhor ao mar. Mas todos são homens de terra; ocupados em dias de semana, fechados em lâmina e papel-máscara—acordados aos controles, presos aos bancos, unidos aos escritórios. Então como é isso? Já que faltaram os campos verdes? O que eles estão aqui?

---

## Qwen2.5 7B Instruct Q4_K_M

**Medido:** 12.53 s de parede na página inteira, 56.4 tokens/s de geração, 671 tokens de saída, custo fixo 210 ms por requisição (174 ms descontando a primeira, que carrega o aquecimento). Carga do modelo: 8.7 s.

### Parágrafo 1 (1107 caracteres de entrada, 339 tokens de saída, 6305 ms de parede, TTFT 263 ms)

> Chamo-me Ishmael. Há alguns anos — nunca me importe dizer exatamente quando —, com pouco ou nenhum dinheiro na bolsa, e sem nada particular para me interessar na terra, pensei que iria navegar um pouco e ver a parte aquática do mundo. É um hábito que tenho de afastar o mau humor e regular a circulação. Cada vez que me encontro a mostrar uma expressão amarga nos lábios; cada vez que é um outubro chuvoso e nublado, e me encontro com a alma um tanto desanimada; cada vez que me encontro involuntariamente a parar diante de depósitos de caixões, e a trazer a última fila de cada enterro que encontro; e, especialmente, cada vez que a minha hypos[1] me dominam tanto, que é necessário um forte princípio moral para me impedir de deliberadamente entrar na rua e a fazer cair as chaves dos outros — então, acho que é altura de me embocar o mais depressa possível. Esta é a minha substituta pelo revólver e pela bala. Com um gesto filosófico, Cato lança-se sobre a espada; eu, tranquilamente, embarco numa embarcação. Não há nada de surpreendente nesta atitude. Se os outros soubessem, quase todos os homens, em graus variados, de vez em quando, nutrem sentimentos muito semelhantes aos que eu nutro para com o oceano.

### Parágrafo 2 (387 caracteres de entrada, 122 tokens de saída, 2323 ms de parede, TTFT 154 ms)

> Agora, tem a sua cidade insular dos Manhattoes, cercada por docas como as ilhas indianas por récifs de coral — o comércio a rodeia com seu mar de ondas. Nas duas direções, as ruas levam você para o mar. Seu bairro mais baixo é o Battery, onde esse nobre molo é lavado pelas ondas e refrescado por brisas que apenas algumas horas antes estavam fora de vista da terra. Olhe para as multidões de observadores de água lá.

### Parágrafo 3 (663 caracteres de entrada, 210 tokens de saída, 3906 ms de parede, TTFT 158 ms)

> Perambule a cidade num domingo ensolarado e sonhador. Vá de Corlears Hook a Coenties Slip, e dali, por Whitehall, para o norte. O que você vê?—Apostados como sentinelas silenciosas por toda a cidade, milhares e milhares de homens mortais se encontram em devaneios marinhos. Alguns apoiados nos pilares; outros sentados nas cabeças de amarração; alguns olhando pelos bulwarks das embarcações de China; outros bem altos nas velas, como se estivessem lutando para dar uma melhor espiada marítima. Mas todos são terraços; aprisionados de dias úteis em pinos e tábuas - presos aos balcões, pregados em bancos, fixados em mesas de trabalho. Mas então, como é isso? Os campos verdes se foram? O que eles fazem aqui?
