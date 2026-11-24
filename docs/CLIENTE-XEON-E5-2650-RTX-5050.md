# Playbook — Xeon E5-2650 + RTX 5050

Caso real de cliente. GPU correta (RTX 50), plataforma de 2012. O objetivo é
tirar o máximo sem prometer o que o hardware não entrega.

## 1. Primeiro, identificar a geração do Xeon

`E5-2650` existe em quatro gerações e elas são **muito** diferentes:

| Versão | Codinome | Ano | Núcleos | Clock base | AVX2 | Memória | PCIe |
|---|---|---|---|---|---|---|---|
| E5-2650 | Sandy Bridge-EP | 2012 | 8C/16T | 2,0 GHz | **não** | DDR3 quad | 3.0 |
| E5-2650 v2 | Ivy Bridge-EP | 2013 | 8C/16T | 2,6 GHz | **não** | DDR3 quad | 3.0 |
| E5-2650 v3 | Haswell-EP | 2014 | 10C/20T | 2,3 GHz | **sim** | DDR4 quad | 3.0 |
| E5-2650 v4 | Broadwell-EP | 2016 | 12C/24T | 2,2 GHz | **sim** | DDR4 quad | 3.0 |

Rode:

```powershell
.\forge.exe detect
```

O campo `ISA` responde na hora: se **não** aparecer `AVX2`, é v1/v2.
Isso decide o build: v1/v2 só têm AVX — e é por isso que o binário **precisa**
ser compilado naquela máquina.

## 2. Resizable BAR (o item de maior impacto)

Chipsets X79/C602 não têm ReBAR de fábrica. Caminho:

1. BIOS: habilitar **Above 4G Decoding** (se existir).
2. Se não existir: **ReBarUEFI** (mod de UEFI) + `UEFIPatch`, aplicando o patch de
   Resizable BAR na ROM da placa-mãe. Requer saber exatamente o modelo da placa e
   ter um dump da BIOS. **É risco real de tijolar** — faça backup do chip e avise o cliente.
3. Verificar:

```powershell
nvidia-smi -q -d MEMORY | Select-String -Pattern "BAR1" -Context 0,2
```

`BAR1 Total` deve ficar próximo do total de VRAM (ex.: 8192 MiB numa placa de 8 GB).
Se ficar em 256 MiB, o ReBAR não está ativo.

## 3. Slot e link PCIe

- Use o **slot x16 ligado direto na CPU**. Em placas X79 o segundo x16 costuma vir
  do chipset ou dividir lanes com M.2/SATA.
- A RTX 5050 é uma placa **x8**: em Gen3 isso dá ~7,9 GB/s.
- Confirme com `forge detect` → linha `Link`. Se aparecer `Gen3 x8` e a placa aceita
  mais, o gargalo é a plataforma, não a GPU.

Expectativa honesta: o passe neural da DLSS 5 fica mais caro nesse link. Por isso o
veredito do `forge` cai de 1080p para **1080p com ressalvas**.

## 4. Memória (Xeon vive de banda, não de latência)

- 4 canais populados e iguais. Com 2 pentes você perde ~metade da banda.
- Se for **dual-socket**, o jogo deve rodar no nó NUMA 0:
  `forge launch` já aplica a afinidade e devolve ao sair.
- Desligue core parking e deixe o plano em Alto desempenho (o launcher faz isso
  temporariamente).

## 5. Ajustes que o `forge plan` vai gerar

Para 8 GB + PCIe 3.0 x8 + tela 1080p:

```
Saida      1920x1080
Interna    960x540  (Desempenho)
Frame Gen  desligado ate validar o frametime
Reflex     On + Boost
VSync      Off (VRR + limite 3 abaixo do refresh)
VRAM       orcamento ~6900 MB
Processo   Above Normal | no NUMA 0 (se dual-socket)
```

No jogo:
- Texturas em **Alto** (Ultra estoura os 8 GB e derruba o 1% low);
- Sombras e geometria em Médio — o Xeon sofre na thread principal;
- Ray tracing: **desligado**. A DLSS 5 já custa caro; somar RT não fecha frame;
- Frame Gen só depois de a base passar de ~45 fps. Frame Gen em cima de 30 fps
  vira input lag, não fluidez.

## 6. Ordem de execução no PC do cliente

```powershell
# 1. compilar ali (nunca copiar)
.\build.ps1

# 2. diagnóstico
.\forge.exe doctor

# 3. corrigir o que o doctor apontar (BIOS/ReBAR/HAGS/VBS) e rodar de novo
.\forge.exe doctor

# 4. perfil + launcher do jogo
.\forge.exe apply --game "D:\SteamLibrary\steamapps\common\Jogo\Jogo.exe"

# 5. jogar pelo launcher gerado (energia/afinidade aplicadas)
```

## 7. Aplicar a DLSS 5 no jogo (o passo que o cliente paga)

```powershell
# uma vez neste PC: baixa OptiScaler + o modelo (~300 MB)
.\tools\fetch-dlss5.ps1

# aplica na pasta que tem o .exe do jogo (Unreal: ...\Binaries\Win64)
.\forge.exe dlss5 --game "D:\SteamLibrary\steamapps\common\Jogo\Binaries\Win64"
.\forge.exe dlss5 --game "D:\SteamLibrary\steamapps\common\Jogo\Binaries\Win64" --yes
```

O `forge` acha o `.exe`, estima o DirectX e procura DLSS do jogo ate 4 niveis
abaixo. Se nao achar, ele **recusa** — e esta certo: o passe neural le a
profundidade e os motion vectors do DLSS do jogo. Copiar DLL num jogo sem DLSS
nao cria nada.

Onde aplica, ele ja deixa `Enabled=true` e `WorkingScale=0.75` (o valor certo
para 8 GB). Num Xeon com PCIe 3.0 x8, se o frametime apertar, use `--scale 0.5`.

No jogo: `Insert` abre o overlay, `F6` compara com/sem DLSS 5. Se o fps desabar,
baixe **Model resolution** no overlay antes de mexer em qualquer outra coisa.

Para voltar ao original (restaura os arquivos que existiam):

```powershell
.\forge.exe dlss5 --game "...\Binaries\Win64" --undo
```

## 8. Quando dizer "nao vale"

- Jogo sem DLSS nenhum: não force. Use DLSS 4.5 / FSR / XeSS — ali o ganho é outro.
- Cliente quer 4K: 8 GB + Gen3 x8 não sustenta. Ofereça 1080p, ou troque a plataforma.
- Frametime instável mesmo em 1080p: o gargalo é o Xeon (2,0–2,6 GHz). Nesse caso o
  ganho vem de CPU (plataforma mais nova), não de DLSS.

Documentar isso por escrito evita devolução e retrabalho.
