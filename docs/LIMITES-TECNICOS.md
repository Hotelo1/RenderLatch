# Limites técnicos — leia antes de prometer ao cliente

## 1. DLSS 5 é um estágio de renderização, não um upscaler

| | DLSS 2/3/4 (Super Resolução) | DLSS 5 |
|---|---|---|
| O que faz | reconstrói uma imagem maior a partir de uma menor | **gera a aparência final** (luz e materiais fotorrealistas) sobre o frame do jogo |
| Entrada | cor + motion vectors + profundidade | frame renderizado, motion vectors, estado temporal, valores de direção de arte |
| Integração | DLL (`nvngx_dlss.dll`) — trocável | **Streamline**, com modelo e máscaras por jogo |
| Hardware | RTX 20/30/40/50 | **somente RTX 50 (Blackwell)** |
| Custo | reduz custo de render | **adiciona** um passe neural por frame |

Consequência prática: a DLSS 5 **não tem fallback de upscaler**. O que existe
hoje é um caminho indireto e honesto: pegar o frame que o jogo já produz e
passá-lo pelo modelo dentro de um gancho de DLL (OptiScaler com o passe DLSSNR,
ou o add-on da RenoDX via ReShade). Isso roda em jogo que **tem** DLSS — é de lá
que vêm profundidade e motion vectors — e não roda em jogo sem DLSS nenhum. E é
um passe **extra**: custa frametime, não devolve.

Fonte: anúncio da NVIDIA na GTC 2026 (março/2026), lançamento no outono de 2026,
paper "DLSS 5: Generative Neural Rendering" (set/2026) e o blog de desenvolvedores
de 22/09/2026.

## 2. Não existe "compilar a DLSS"

O que a NVIDIA entrega:
- `nvngx_dlss*.dll` e as DLLs do Streamline (`sl.dlss.dll`, `sl.dlss_g.dll`, `sl.reflex.dll`);
- modelos neurais como dados opacos, assinados;
- nenhum código-fonte, nenhuma permissão de redistribuição fora do SDK.

O que isso significa:
- `-march=native` não toca em um byte da inferência. Ela roda em kernels CUDA/tensor
  já compilados para SM 12.0 (Blackwell).
- Quem diz "compilei a DLSS no meu PC e ganhei 30%" está medindo outra coisa
  (preset, resolução, driver, ou placebo).

**O que dá para compilar por PC, de verdade:**

| Camada | Compilável? | Ganho típico |
|---|---|---|
| Nosso tuner/launcher (Rust) | sim | irrelevante em FPS, relevante em overhead e confiabilidade |
| Camadas abertas (OptiScaler, DXVK, vkBasalt) | sim (GPL) | de 0% a 10%, depende do jogo |
| Shaders do jogo | só se o jogo expõe | varia muito |
| Modelo DLSS 5 | **não** | — |

## 3. Onde o ganho real mora (por ordem de impacto)

1. **Resizable BAR.** Em RTX 50, BAR1 pequeno aumenta round-trips de memória.
   Ganho observado costuma ficar entre 3% e 10%, com queda forte de stutter perto do
   limite de VRAM. Em placa antiga (X79/C602) exige ReBarUEFI.
2. **Link PCIe.** RTX 5050 é uma placa x8. Em slot Gen3 isso dá ~7,9 GB/s (metade de
   um Gen4 x8). O passe neural da DLSS 5 troca buffers grandes por frame: link curto
   aumenta o custo e piora o 1% low.
3. **VRAM.** A DLSS 5 é descrita como custosa em VRAM (a demo de lançamento usou
   **duas** RTX 5090: uma para o jogo, outra só para a DLSS 5). Em 8 GB, 4K não fecha.
   Orçar 85% da VRAM para o jogo é o teto seguro.
4. **VBS/HVCI desligados.** 3% a 12% em jogos, mais em CPU antiga.
5. **HAGS ligado.** Frame Gen e pipelines neurais se beneficiam do agendamento por hardware.
6. **Energia/afinidade/prioridade.** Não aumenta FPS médio, mas segura o frametime —
   que é o que o jogador sente.
7. **Resolução interna e preset.** O maior botão de todos, e o mais barato.

## 4. Por que "qualquer jogo" tem letra miúda

- A DLSS 5 nativa exige que o engine forneça buffers e motion vectors no formato do
  Streamline, e que o desenvolvedor escolha o modelo e as máscaras.
- Na rota OptiScaler do `forge dlss5`, o passe usa os dados que o jogo já
  entrega ao DLSS a cada frame.
- Jogos confirmados com DLSS 5 nativa incluem NBA 2K27 (já disponível),
  Assassin's Creed Shadows, Starfield, Hogwarts Legacy, Resident Evil Requiem,
  Oblivion Remastered, Delta Force, NARAKA, Where Winds Meet.
- Para jogos sem DLSS nativo, a rota Feeder usa ReShade, DLSS5-Feeder e LumeniteFX
  para preparar a profundidade e os vetores de movimento. É uma integração
  experimental com o DLSS5oneclick. A compatibilidade depende do jogo,
  da API gráfica e da GPU.
- Anti-cheat: DLL extra no diretório do jogo pode ser bloqueada ou interpretada
  como fraude. Use em jogo offline. O `forge` avisa antes de copiar.

## 5. O que o Forge faz com isso

- mede e classifica a máquina (não adivinha);
- rebaixa o veredito quando o link ou a VRAM não sustentam a resolução;
- impede o usuário de ligar Frame Gen em cima de um frametime que já está ruim;
- compila o binário com o conjunto de instruções **daquela** CPU;
- deixa o jogo abrir com energia/afinidade/prioridade corretas e restaura tudo ao sair;
- **aplica a DLSS 5 sob demanda** (`forge dlss5`): escolhe a escala do modelo pela
  VRAM, faz backup, registra o que colocou e desfaz com `--undo`;
- por padrão não toca em arquivo de jogo. Só o `dlss5` copia DLL para a pasta do
  jogo — e nesse caso o aviso de anti-cheat aparece antes.

## 6. Como falar com o cliente

> "A DLSS 5 é da NVIDIA e roda em RTX 50. Eu preparo o seu PC para ela — meço o que
> a sua placa-mãe entrega, corrijo ReBAR, PCIe e energia, e calculo a resolução e
> o preset que o seu hardware aguenta. Depois, se o jogo já tem DLSS, eu ligo a
> DLSS 5 nele com um comando, com backup e botão de desfazer. No seu caso isso é
> 1080p: em 4K a placa de 8 GB não fecha o orçamento, e o passe neural ainda tira
> fps. É qualidade de imagem, não ganho de desempenho."

Prometer 4K numa RTX 5050 de 8 GB com PCIe 3.0 é vender devolução.
