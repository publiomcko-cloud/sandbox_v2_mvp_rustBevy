# MVP B — Rust + Bevy 0.20 + GPU Compute

Status: **especificação de implementação do zero; não implementado**. 18/11/2026.
Critérios comuns e testes: `CRITERIOS_COMUNS.md`. Repositório próprio, novo e independente: `sandbox_v2_mvp_rustBevy`.

## Proposta de validação

Comprovar que terrenos editáveis volumétricos podem ser gerados e exibidos com geometria residente na GPU, sem duplicar toda a extração/mesh em CPU e sem reconstruir cena ou transferir arrays maciços a cada atualização. O uso de Bevy não fornece automaticamente terreno voxel, stream ou GPU mesh; estas funcionalidades são parte deste MVP.

## Stack congelada na primeira baseline

- Rust stable registrado no `rust-toolchain.toml`; Bevy **0.20.x** (commit/versão exata no Cargo.lock), wgpu de versão compatível com Bevy, shaders WESL/WGSL.
- `avian3d 0.8` para Bevy 0.20 (conferir e fixar versões realmente compiláveis); `glam`/matemática de Bevy, `rayon` ou task pool nativo para CPU, `serde` + `bincode` para persistência de estados e testes.
- Alvo inicial Windows/Vulkan (eventual DX12 configurável). `mesh shaders` NÃO são pré-requisito; usar compute shader + storage/index buffers + desenho indireto via pipeline de render próprio compatível com wgpu. Uso de mesh shaders apenas depois de uma baseline portável válida.

## Organização sugerida

```
sandbox_v2_mvp_rustBevy/
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  assets/shaders/sdf_sample.wgsl
  assets/shaders/active_cells.wgsl
  assets/shaders/emit_mesh.wgsl
  assets/shaders/cull.wgsl
  assets/shaders/water.wgsl
  crates/world_core/src/{seed,sdf,edits,persistence}.rs
  crates/streaming/src/{scheduler,residency,version}.rs
  crates/gpu_terrain/src/{buffers,compute,render_plugin,indirect}.rs
  crates/physics/src/{terrain_collision,rocks,water}.rs
  crates/game/src/{main,player,hud}.rs
  crates/bench/src/{routes,metrics,report}.rs
  tests/{geometry,stale_results,streaming,edit_latency,save_reopen}.rs
  reports/
```

## Subsistemas

**World Core (CPU).** Função SDF determinística com seed, relevo e cavernas. Identidade de bloco por coordenadas inteiras 64-bit; deslocamento de origem da câmera ou transforms relativos para precisão; edições em estrutura esparsa por bloco, persistidas separadamente. O CPU guarda as revisões e campos editados, mas não mantém cópias de todas as malhas visuais. Não fazer varredura global de todas as regiões para resolver edições locais.

**Blocos e LOD.** Para o MVP: blocos lógicos de **16m de lado**, grade 32³ em LOD0 (0,5m), 16³ em LOD1 (1m), 8³ em LOD2 (2m). Gerar apenas blocos que interceptem solo/caverna/área visível; classificação por limites e amostras, com cautela para não eliminar estruturas finas. Streaming até raio horizontal de 130m, banda vertical esparsa ao redor do solo, câmera podendo entrar em cavernas. Bordas compartilhadas com halo de amostras e esquema explícito de transição LOD: usar células Transvoxel se Marching Cubes e integração viável; caso contrário início com Surface Nets + mesma resolução entre vizinhos até possuir solução de seams comprovada. Não aceitar "skirt" que exponha rachaduras em entradas de cavernas/edições.

**GPU compute.** Pacote de N blocos por dispatch (medir N=8,16,32,64): `density -> occupancy/classification -> prefix-sum/compaction -> triangles/vertices -> culling + indirect args`. Buffer pool persistente por bloco/LOD com ring/allocator e quota VRAM; dados residentes acessados pelo vertex shader/indirect draw no mesmo dispositivo GPU, sem GPU→CPU bulk readback. A CPU envia somente comandos compactos (seeds, bloco, versão e deltas editados) e recebe contadores/estatísticas assíncronos opcionais. Evitar despacho minúsculo por bloco se lotes são mais econômicos. Adotar budget e overflow explícitos: se um bloco gerar mais vértices que sua reserva, requisitar espaço, preservar versão anterior, reprocessar com overflow controlado e medir atraso.

**Publicação sem gaps.** Para cada bloco, `ResidentVisible(rev=R)` permanece desenhado enquanto `Building(rev=R+1)` ocupa outro buffer. Após completar todas as etapas e sincronização GPU/fence, trocar ponteiro/desenho somente se revisão atual ainda é R+1. Descartar resultados obsoletos SEM retirar rev R. Não apagar visual antigo simplesmente porque há job em andamento. A edição só está aprovada se R+1 tornar-se visível em no máximo 100ms, inclusive quando o buffer antigo está retido. Terrain/rocks/water devem ser apresentados de forma coordenada no primeiro carregamento; objetos distantes não podem aparecer em vazio.

**Colisão CPU próxima.** Gerar colliders simplificados somente no raio de 24–32m e atualizar assincronamente por revisões. Usar CPU amostrando o mesmo SDF + deltas, e validar erro de paridade CPU/GPU nas superfícies: mesma semântica de seed, quantização e sinal, tolerância espacial explicitada. Nunca usar readback de toda a malha visual para cada colisor. Física com timestep fixo. Inspecionar divergência de altura colisão × visual e prevenir tunnelling com CCD/colisores adequados.

**Rochas.** Posições determinísticas geradas da seed e consulta de superfície; renderizar 400+ instâncias dentro do horizonte visual por instancing GPU. Até ~24–32m, promover apenas objetos em interação a corpos rígidos Avian com collider convexo, gravidade, atrito, repouso. Estado promovido é persistido; despromoção quando fora do alcance e imóvel, sem reposicionar abruptamente. Se tocar água, aplicar empuxo, arrasto, corrente. Densidade de rocha maior que água: deve afundar.

**Água.** Superfície oceânica/lago em y=0, faixa de rio simplificada (spline/polígonos sem simulação de fluido); renderização própria, sem blocos independentes trocados junto com terreno. Máscara de costa baseada em profundidade e edição SDF, com atualização local sincronizada. `WaterVolume` determina submersão e velocidade de corrente; solver básico `F_b = rho * g * V_deslocado` + arrasto aproximado; personagem pode boiar/nadar conforme parâmetros e rocha afunda. A água não gera colisores sólidos de superfície. Sem hidráulica conservativa, erosão ou simulação voxel de água.

**Backpressure.** Janela de 130m pronta e apresentação priorizada; 36m extra de prefetch direcional a 18m/s quando houver capacidade; limites separados de bytes GPU, blocos em geração, GPU dispatch/frame, CPU colisores, VRAM, uploads de edições e corpo rígidos. Não exceder limite para obter benchmark artificial. Cache deve ser dimensionado para não provocar churn no fluxo normal.

## Execução incremental e gates

| Fase | Trabalho | Gate antes de prosseguir |
|---|---|---|
| B0 | Bevy + Avian + pipeline compute/render mínimo, CI Windows | Build release e tempo de frame cena trivial |
| B1 | SDF Rust + paridade WGSL + campo de densidade por bloco | Testes numéricos e caves corretas |
| B2 | Compute GPU + extração + desenho residente sem readback | Buffers desenhados diretamente pela GPU; sem upload por cada frame |
| B3 | Streaming, 130m, transições LOD e retenção | Sem seams, buracos ou flicker real a 18m/s |
| B4 | Edição persistente e swap revision-gated com 100ms | Testes clique→pixel, bordas e save/reopen |
| B5 | Física de colisores próximos, rochas e água | Solidez física e sincronismo visual |
| B6 | Benchmarks N dispatch e perf, RAM/VRAM, rotas | Medir custo total, inclusive CPU/GPU/pressão de memória |
| B7 | Bateria completa conforme contrato comum | PASS/FAIL explícito para cada meta |

## Hipóteses eliminatórias

- Se GPU mesh direta não for implementada e o programa ler todo o mesh de volta para CPU, não há prova válida de arquitetura GPU-first; marcar bloqueio arquitetural.
- Se LOD gerar fissuras, não esconder com neblina, downgrade espontâneo ou recorte da distância. Implementar costura ou mesmo LOD temporário e medir custo.
- Readback síncrono por bloco e alocações intensas de GPU buffers por frame são proibidos no caminho quente.
- Edição em 100ms tem de incluir recomputar SDF, compaction, desenho e sincronizar a máscara de água local; overflows de buffer não justificam latência indefinida.
- Se CPU e GPU discordarem na superfície a ponto de gerar flutuação de rochas, colisão ruim ou cracks, a interface de dados e as amostras devem ser corrigidas antes de otimização.

## Done

Executável Windows Release, 130m visuais, 18m/s sem gaps, cavernas e brushes add/sub, rochas físicas e água com empuxo/arrasto, stream persistente e testes com os quatro gates, exportando logs e vídeo em rotas determinísticas; buffers de geometria produzidos/desenhados em GPU com colisor local CPU.

Fontes: https://bevy.org/news/bevy-0-20/ ; https://github.com/bevyengine/bevy/blob/main/examples/shader_advanced/compute_mesh.rs ; https://docs.rs/crate/avian3d/latest ; https://docs.rs/bevy/latest/bevy/render/struct.RenderPlugin.html