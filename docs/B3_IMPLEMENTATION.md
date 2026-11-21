# B3 — Streaming 130 m e retenção de geometria GPU (candidato)

Esta fase parte de B2 e usa blocos lógicos de 16 m, prioridade por distância até AABB
(realmente esférica no plano horizontal, inclusive na diagonal), 130 m
de horizonte 360°, 36 m de prefetch direcional, 182 m de retenção e
12 blocos de geração por frame. Duas camadas verticais esparsas abrangem
solo e caverna. A cota de residência inicial é 1.100 blocos.

Os blocos entram ocultos; só são publicados após o registro das
etapas GPU de densidade, vértices e índices; na saída da janela de
retenção, seus assets e buffers de densidade são liberados. Não existe
readback de malha na execução normal.

**Política provisória de segurança:** LOD2 uniforme para todo o horizonte.
LOD0/LOD1 existem no B2, mas a transição entre resoluções distintas
ainda NÃO foi implementada ou qualificada. Sem skirts ou fog para
esconder rachaduras. A B3 não estará aprovada até demonstrar transição
LOD real, 130 m completos, ausência de gaps a 18 m/s e frame completo
dentro do orçamento.

Comandos no PowerShell, pasta do artifact extraído:

    $env:SANDBOX_STAGE = "B3"
    $env:WGPU_BACKEND = "vulkan"
    .\sandbox-game.exe

Rota determinística automatizada (headless):

    $env:SANDBOX_STAGE = "B3"
    $env:SANDBOX_B3_HEADLESS = "1"
    $env:SANDBOX_B3_ROUTE = "straight"
    $env:SANDBOX_B2_CAPTURE_PATH = "$PWD\b3-route.png"
    .\sandbox-game.exe

A rota aguarda 5 segundos de warmup e avança a 18 m/s em +X.
O log "B3 horizon coverage" indica required, visible, missing, resident
e evictions. Uma execução visual estática não é comprovação de 60 FPS
em deslocamento. Os critérios globais não devem ser promovidos por
inferência.

## Gates

1. Rustfmt e testes puros de planejamento no Linux;
2. cargo check Windows do jogo COMPLETO antes do Release caro;
3. Release e empacotamento somente se check passar;
4. execução headless GPU na GTX 1050 Ti, captura e telemetria 130 m;
5. avaliação de gaps e FPS de 18 m/s, e transições LOD reais;
6. B4 cuidará da persistência/edits revision-gated.
