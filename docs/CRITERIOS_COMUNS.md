# Contrato de aceitação — dois MVPs sandbox independentes

Status: **especificação; nenhuma implementação ou benchmark foi executado**. Data: 2026-11-18.

## 1. Escopo funcional obrigatório

- Mundo procedural tridimensional determinístico por seed, expansível, com relevo contínuo, depressões e cavernas atravessáveis; edição SDF de subtração e adição em execução, inclusive na borda entre blocos.
- Controle de câmera/personagem, deslocamento horizontal constante de 18 m/s, colisões locais, capacidade de entrar/sair da água e editar terreno durante movimento.
- **Raio visual de 130 m no plano horizontal**, em todas as direções, com LOD e geometria completa de solo visível (não somente distância de agendamento). Objetos e água devem acompanhar o terreno. Fora do raio, qualidade e cobertura são opcionais.
- Rochas procedurais determinísticas: modelo instanciado distante, física rígida ao aproximar/interagir (gravidade, atrito, rotação, contato e repouso); identidade/estado preservado após interação.
- Água: oceano ou lago em y=0, um curso de água com velocidade prescrita; superfície sem blocos aparentes; interação com personagem (entrada, nado/arrasto) e objetos (empuxo, arrasto, densidade); **não** exigir hidrodinâmica conservativa, erosão, fluidos 3D ou propagação física de ondas. Rocha mais densa que água deve afundar. Manter correspondência visual com costa após edição local.
- Persistência mínima: mundo definido pela seed e versão, edições locais, estados modificados de rochas. Fechar/reabrir preserva as alterações.
- Sem assets externos sofisticados, multiplayer, clima complexo, plantas ou UI avançada. Inspeção em 1080p e câmera com FOV vertical padronizado.

## 2. Mesma especificação de carga, implementações separadas

- Executáveis independentes com repositórios próprios: `sandbox_v2_mvp_godotVoxel` e `sandbox_v2_mvp_rustBevy`; não incorporar código ou dependências de qualquer projeto anterior. A comparação compartilha apenas esta especificação e um conjunto de seeds/caminhos de teste.
- Mundo de referência: colinas e vales, região abaixo da cota zero inundada, ao menos uma caverna contínua, uma encosta íngreme, pelo menos 400 rochas distribuídas visivelmente na área de 130 m de um ponto de referência, com pelo menos 24 rochas capazes de tornar-se dinâmicas na região interativa. Para comparação, padronizar posições, quantidades e dificuldade geométrica por arquivos de cenário, sem exigir malhas binariamente idênticas.
- Qualidade visual inicial: voxel equivalente a 0,5 m próximo ao personagem (até aproximadamente 32 m), LOD intermediário até 80 m e mais distante até 130 m, sem rachaduras perceptíveis. A discretização exata pode divergir entre motores se não afetar os critérios visuais.
- Cobertura inicial finalizada antes de liberar o controle em um mundo frio; medir e informar separadamente o tempo de inicialização. **Não mascarar falhas de streaming durante movimento com teleporte, neblina opaca ou pausa automática do jogador.**
- Janela alvo adicional de preparação/prefetch: 36 m para 2 s à frente de 18 m/s, quando viável. Não confundir prefetch solicitado com terreno realmente pronto.

## 3. Critérios de aceite — quatro metas inegociáveis

| ID | Meta | Regra de medição / aprovação |
|---|---|---|
| PERF-60 | **60 FPS, 16,7 ms por frame completo** | Capturar duração ponta a ponta incluindo CPU principal, render thread, execução GPU e apresentação, em 1920×1080 nativos. Meta estrita: nenhum frame **induzido pela aplicação** acima de 16,7 ms nos percursos qualificados; reportar máximo, p50, p95, p99 e p99,9, inclusive frames >20/33 ms. Eventos de SO comprovados são registrados e repetidos, nunca apagados silenciosamente. O critério não é substituído por média de FPS. |
| STREAM-18 | **18 m/s contínuos, zero lacunas visuais** | Rota reta longa, curvas, reversões, retorno por caminho aquecido e cruzamento repetido de fronteiras de LOD/bloco; sem terreno piscando/desaparecendo, rochas flutuantes por atraso, água ausente ou seam visível. Separar fluxo frio e retorno quente. |
| VIEW-130 | **130 m de raio visual** | Testar 360° a partir de pontos representativos, com verificação geométrica por profundidade e revisão gráfica. Medir cobertura efetivamente apresentada, não somente contagem de requests/nós. Cobertura tem de incluir solo, rochas e água pertinentes. |
| EDIT-100 | **≤ 100 ms até edição local VISÍVEL** | Cronometrar do input aceito ao primeiro frame realmente apresentado com resultado correto. Realizar adição/subtração, múltiplos cliques e edição na borda enquanto se move. Resultado tardio reprova mesmo que o worker tenha terminado no prazo. |

**Meta adicional de segurança:** colisão atualizada após edição sem atravessar terreno; registrar atraso de colisores separadamente. Tratar qualquer interpenetração perceptível ou queda de player/rocha através do solo como falha funcional.

## 4. Instrumentação e qualidade de evidência

- Ambos medem: `request->queue`, preparação de densidade, extração de superfície, upload/commit, frame de visibilidade, recriação/remoção de buffers, custo de colisores, GPU compute e render, memória residente/VRAM, tamanho de backlog, revisões descartadas, ocupação do horizonte, tempo de atualização de rochas e água.
- A mesma carga de instrumentação deve permanecer nos dois braços de qualquer teste A/B; distinguir teste headless de teste gráfico real. A câmera e o input são reproduzidos por caminho/timestamp determinísticos, não pelo teclado humano.
- Rotas de aceite: `stationary_180s`, `straight_18mps_600s`, `turns_18mps_300s`, `out_and_back_18mps_300s`, `shoreline_180s`, `cave_180s`, `edit_idle_100`, `edit_moving_100`, `boundary_edit_100`, `rocks_water_180s`, `reopen_save`, `stress_54mps` (stress informativo, NÃO gate).
- Executar cada rota qualificatória três vezes com world seed fixa + pelo menos duas seeds adicionais; manter resolução, drivers, hardware, qualidade e instrumentação idênticos. Registrar versões exatas do compilador, engine e dependências.
- Guardas por estágio: testes unitários de geração, invariantes de fronteira e revisão, visual real, resultado de arquivo de save, carga de recursos, estabilidade de memória. Nenhum benchmark sozinho autoriza aceite.

## 5. Invariantes arquiteturais

1. Dados de jogo e seu estado persistente não dependem da existência do objeto gráfico.
2. Nunca esconder/destruir a apresentação válida antes que a substituição completa tenha passado por validação de revisão e esteja pronta para apresentação. No caso de edição, a geometria antiga só pode permanecer pelo prazo de latência permitido.
3. Um sistema é responsável por publicar cada item visual; recursos/água/rochas não devem antecipar seu terreno-base ao criar regiões novas.
4. Resultados de tarefas antigas são descartados sem apagar uma apresentação válida. Invalidação é espacial, limitada ao volume afetado.
5. Não sincronizar GPU→CPU para toda geometria visual; consultas de física usam representação local independente ou readback pequeno e assíncrono controlado.
6. Definir limites para memória, work admission, CPU/GPU por frame e número de corpos rígidos; backpressure previsível. Não impor limite de cache inferior ao conjunto útil de trabalho.
7. Physics fixed tick distinto da frequência de render; não fazer alocação de cena/meshes individuais para toda rocha distante.
8. Nenhuma aprovação baseada apenas em estado de SceneTree ou disponibilidade de buffers; confirmar pixels efetivamente visíveis em renderização real.

## 6. Resultado esperado de cada MVP

- Binário desktop Windows executável, fontes e build reprodutível, script de benchmark determinístico, configurações gráficas fixas, três relatórios de execução A/B, vídeo de prova visual e uma tabela com PASS/FAIL em cada critério.
- Relatório separa o que está implementado, medido, reprovado, não testado, e o que é mera hipótese. Cada MVP pode falhar honestamente; escolher com base no resultado comparável, não em promessa de velocidade.

## 7. Fontes técnicas de referência

- Voxel Tools VoxelLodTerrain: https://voxel-tools.readthedocs.io/en/latest/api/VoxelLodTerrain/
- Voxel Tools performance: https://voxel-tools.readthedocs.io/en/latest/performance/
- Voxel Tools streams: https://voxel-tools.readthedocs.io/en/latest/streams/
- Voxel Tools instancing: https://voxel-tools.readthedocs.io/en/latest/instancing/
- Bevy 0.20: https://bevy.org/news/bevy-0-20/
- Bevy GPU compute-to-mesh example: https://github.com/bevyengine/bevy/blob/main/examples/shader_advanced/compute_mesh.rs
- Avian Physics: https://docs.rs/crate/avian3d/latest