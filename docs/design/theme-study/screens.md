## Offline rendered screens and style callouts

All frames below come from `ratatui::backend::TestBackend`, not a terminal or gateway. Coordinates are zero-based (column,row). Blank trailing cells are omitted from text, but buffers retain their full dimensions. Plain Markdown cannot display cell colors: the callouts and swatch identifiers preserve their exact mapping.

### Ember / dark warm high contrast

Role colors below are the actual preview colors. Element backgrounds inherit their area: background for transcript/composer/status/tool/diff; surface for menu/work/dialog/picker/lists/code/queue/alerts/badges and alternate views. Selection and focus use the accent background and background foreground, or terminal reverse in Terminal. Attributes come from the catalogue.

| Role | Foreground | Base background |
|---|---|---|
| background | #f5eadb | #171411 |
| surface | #f5eadb | #25201b |
| text | #f5eadb | #171411 |
| muted | #c4b5a2 | #171411 |
| disabled | #a19383 | #171411 |
| border | #9c846b | #171411 |
| primary | #ffc078 | #171411 |
| accent | #ffb86b | #171411 |
| success | #a8d08d | #171411 |
| warning | #ffd166 | #171411 |
| error | #ff8b82 | #171411 |
| attention | #ffd166 | #171411 |
| selection | #171411 | #ffb86b |
| focus | #171411 | #ffb86b |

#### 80x24: menu on, panel opened on demand as overlay

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                               │Work / 1 needs you
 You: Keep the interface clean and light.      │Goal: keep the interface light
 Talaria                                       │Turn: running / compare themes
 A few overrides can change the whole theme.   │Verdict: pending
 Theme preview                                 │Tasks 1/3
 Reasoning: checking readable contrast.        │[x] Read inputs
 Read docs/themes.md                           │[>] Compare themes
 rust / sample.rs                              │[ ] Review screens
 let theme = palette.resolve();                │Agents
 --- sample.rs                                 │! review: needs you
 - old_color                                   │  model: unknown / effort: ?
 + semantic_role                               │> preview: rendering
 ok render: six example workspace frames       │  model: unknown / effort: ?
 ! Preview only; decision pending.             │- next: queued
                                               │ok inventory: done
                                               │+2 more
                                               │Alt+S details / Alt+A next
                                               │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                   │
╰──────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.overlay.border` (47,1); `work.attention` (48,1); `work.verdict.pending` (48,4); `queue.text` (0,19); `composer.border.focused` (0,20); `composer.marker` (2,21); `status.voice.phase` (42,23); styles resolve through the catalogue and role table above.

#### 80x24: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016
0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032
0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064
0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080
0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112
0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128
0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160
0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176
0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208
0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224
0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256
0257 0258 0259 0260 0261 0262 0263 0264 0265 0266 0267 0268 0269 0270 0271 0272
0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304
0305 0306 0307 0308 0309 0310 0311 0312 0313 0314 0315 0316 0317 0318 0319 0320
0321 0322 0323



```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 16), y = floor((n − 1) / 16). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

#### 120x40: menu on, panel docked

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                                                                       │Work / 1 needs you
 You: Keep the interface clean and light.                                              │Goal: keep the interface light
 Talaria                                                                               │Turn: running / compare themes
 A few overrides can change the whole theme.                                           │Verdict: pending
 Theme preview                                                                         │Tasks 1/3
 Reasoning: checking readable contrast.                                                │[x] Read inputs
 Read docs/themes.md                                                                   │[>] Compare themes
 rust / sample.rs                                                                      │[ ] Review screens
 let theme = palette.resolve();                                                        │Agents
 --- sample.rs                                                                         │! review: needs you
 - old_color                                                                           │  model: unknown / effort: ?
 + semantic_role                                                                       │> preview: rendering
 ok render: six example workspace frames                                               │  model: unknown / effort: ?
 ! Preview only; decision pending.                                                     │- next: queued
                                                                                       │ok inventory: done
                                                                                       │+2 more
                                                                                       │Alt+S details / Alt+A next
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                                                           │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.divider` (87,1); `work.attention` (88,1); `work.verdict.pending` (88,4); `queue.text` (0,35); `composer.border.focused` (0,36); `composer.marker` (2,37); `status.voice.phase` (42,39); styles resolve through the catalogue and role table above.

#### 120x40: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024
0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072
0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120
0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168
0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216
0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256 0257 0258 0259 0260 0261 0262 0263 0264
0265 0266 0267 0268 0269 0270 0271 0272 0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304 0305 0306 0307 0308 0309 0310 0311 0312
0313 0314 0315 0316 0317 0318 0319 0320 0321 0322 0323


























```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 24), y = floor((n − 1) / 24). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

### Paper / light

Role colors below are the actual preview colors. Element backgrounds inherit their area: background for transcript/composer/status/tool/diff; surface for menu/work/dialog/picker/lists/code/queue/alerts/badges and alternate views. Selection and focus use the accent background and background foreground, or terminal reverse in Terminal. Attributes come from the catalogue.

| Role | Foreground | Base background |
|---|---|---|
| background | #25231f | #fffcf5 |
| surface | #25231f | #eee8dc |
| text | #25231f | #fffcf5 |
| muted | #665d50 | #fffcf5 |
| disabled | #766a58 | #fffcf5 |
| border | #897a65 | #fffcf5 |
| primary | #714000 | #fffcf5 |
| accent | #005a80 | #fffcf5 |
| success | #285c2a | #fffcf5 |
| warning | #704600 | #fffcf5 |
| error | #a01824 | #fffcf5 |
| attention | #704600 | #fffcf5 |
| selection | #fffcf5 | #005a80 |
| focus | #fffcf5 | #005a80 |

#### 80x24: menu on, panel opened on demand as overlay

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                               │Work / 1 needs you
 You: Keep the interface clean and light.      │Goal: keep the interface light
 Talaria                                       │Turn: running / compare themes
 A few overrides can change the whole theme.   │Verdict: pending
 Theme preview                                 │Tasks 1/3
 Reasoning: checking readable contrast.        │[x] Read inputs
 Read docs/themes.md                           │[>] Compare themes
 rust / sample.rs                              │[ ] Review screens
 let theme = palette.resolve();                │Agents
 --- sample.rs                                 │! review: needs you
 - old_color                                   │  model: unknown / effort: ?
 + semantic_role                               │> preview: rendering
 ok render: six example workspace frames       │  model: unknown / effort: ?
 ! Preview only; decision pending.             │- next: queued
                                               │ok inventory: done
                                               │+2 more
                                               │Alt+S details / Alt+A next
                                               │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                   │
╰──────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.overlay.border` (47,1); `work.attention` (48,1); `work.verdict.pending` (48,4); `queue.text` (0,19); `composer.border.focused` (0,20); `composer.marker` (2,21); `status.voice.phase` (42,23); styles resolve through the catalogue and role table above.

#### 80x24: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016
0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032
0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064
0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080
0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112
0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128
0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160
0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176
0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208
0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224
0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256
0257 0258 0259 0260 0261 0262 0263 0264 0265 0266 0267 0268 0269 0270 0271 0272
0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304
0305 0306 0307 0308 0309 0310 0311 0312 0313 0314 0315 0316 0317 0318 0319 0320
0321 0322 0323



```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 16), y = floor((n − 1) / 16). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

#### 120x40: menu on, panel docked

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                                                                       │Work / 1 needs you
 You: Keep the interface clean and light.                                              │Goal: keep the interface light
 Talaria                                                                               │Turn: running / compare themes
 A few overrides can change the whole theme.                                           │Verdict: pending
 Theme preview                                                                         │Tasks 1/3
 Reasoning: checking readable contrast.                                                │[x] Read inputs
 Read docs/themes.md                                                                   │[>] Compare themes
 rust / sample.rs                                                                      │[ ] Review screens
 let theme = palette.resolve();                                                        │Agents
 --- sample.rs                                                                         │! review: needs you
 - old_color                                                                           │  model: unknown / effort: ?
 + semantic_role                                                                       │> preview: rendering
 ok render: six example workspace frames                                               │  model: unknown / effort: ?
 ! Preview only; decision pending.                                                     │- next: queued
                                                                                       │ok inventory: done
                                                                                       │+2 more
                                                                                       │Alt+S details / Alt+A next
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                                                           │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.divider` (87,1); `work.attention` (88,1); `work.verdict.pending` (88,4); `queue.text` (0,35); `composer.border.focused` (0,36); `composer.marker` (2,37); `status.voice.phase` (42,39); styles resolve through the catalogue and role table above.

#### 120x40: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024
0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072
0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120
0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168
0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216
0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256 0257 0258 0259 0260 0261 0262 0263 0264
0265 0266 0267 0268 0269 0270 0271 0272 0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304 0305 0306 0307 0308 0309 0310 0311 0312
0313 0314 0315 0316 0317 0318 0319 0320 0321 0322 0323


























```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 24), y = floor((n − 1) / 24). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

### Terminal / ANSI

Role colors below are the actual preview colors. Element backgrounds inherit their area: background for transcript/composer/status/tool/diff; surface for menu/work/dialog/picker/lists/code/queue/alerts/badges and alternate views. Selection and focus use the accent background and background foreground, or terminal reverse in Terminal. Attributes come from the catalogue.

| Role | Foreground | Base background |
|---|---|---|
| background | default | default |
| surface | default | default |
| text | default | default |
| muted | Gray | default |
| disabled | DarkGray | default |
| border | Gray | default |
| primary | Yellow | default |
| accent | Cyan | default |
| success | Green | default |
| warning | Yellow | default |
| error | Red | default |
| attention | Yellow | default |
| selection | default | default |
| focus | default | default |

#### 80x24: menu on, panel opened on demand as overlay

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                               |Work / 1 needs you
 You: Keep the interface clean and light.      |Goal: keep the interface light
 Talaria                                       |Turn: running / compare themes
 A few overrides can change the whole theme.   |Verdict: pending
 Theme preview                                 |Tasks 1/3
 Reasoning: checking readable contrast.        |[x] Read inputs
 Read docs/themes.md                           |[>] Compare themes
 rust / sample.rs                              |[ ] Review screens
 let theme = palette.resolve();                |Agents
 --- sample.rs                                 |! review: needs you
 - old_color                                   |  model: unknown / effort: ?
 + semantic_role                               |> preview: rendering
 ok render: six example workspace frames       |  model: unknown / effort: ?
 ! Preview only; decision pending.             |- next: queued
                                               |ok inventory: done
                                               |+2 more
                                               |Alt+S details / Alt+A next
                                               |
 Queued 1: compare the light variant
+------------------------------------------------------------------------------+
| > Keep the defaults quiet.                                                   |
+------------------------------------------------------------------------------+
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.overlay.border` (47,1); `work.attention` (48,1); `work.verdict.pending` (48,4); `queue.text` (0,19); `composer.border.focused` (0,20); `composer.marker` (2,21); `status.voice.phase` (42,23); styles resolve through the catalogue and role table above.

#### 80x24: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016
0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032
0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064
0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080
0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112
0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128
0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160
0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176
0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208
0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224
0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256
0257 0258 0259 0260 0261 0262 0263 0264 0265 0266 0267 0268 0269 0270 0271 0272
0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304
0305 0306 0307 0308 0309 0310 0311 0312 0313 0314 0315 0316 0317 0318 0319 0320
0321 0322 0323



```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 16), y = floor((n − 1) / 16). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

#### 120x40: menu on, panel docked

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                                                                       |Work / 1 needs you
 You: Keep the interface clean and light.                                              |Goal: keep the interface light
 Talaria                                                                               |Turn: running / compare themes
 A few overrides can change the whole theme.                                           |Verdict: pending
 Theme preview                                                                         |Tasks 1/3
 Reasoning: checking readable contrast.                                                |[x] Read inputs
 Read docs/themes.md                                                                   |[>] Compare themes
 rust / sample.rs                                                                      |[ ] Review screens
 let theme = palette.resolve();                                                        |Agents
 --- sample.rs                                                                         |! review: needs you
 - old_color                                                                           |  model: unknown / effort: ?
 + semantic_role                                                                       |> preview: rendering
 ok render: six example workspace frames                                               |  model: unknown / effort: ?
 ! Preview only; decision pending.                                                     |- next: queued
                                                                                       |ok inventory: done
                                                                                       |+2 more
                                                                                       |Alt+S details / Alt+A next
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
 Queued 1: compare the light variant
+----------------------------------------------------------------------------------------------------------------------+
| > Keep the defaults quiet.                                                                                           |
+----------------------------------------------------------------------------------------------------------------------+
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.divider` (87,1); `work.attention` (88,1); `work.verdict.pending` (88,4); `queue.text` (0,35); `composer.border.focused` (0,36); `composer.marker` (2,37); `status.voice.phase` (42,39); styles resolve through the catalogue and role table above.

#### 120x40: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024
0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072
0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120
0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168
0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216
0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256 0257 0258 0259 0260 0261 0262 0263 0264
0265 0266 0267 0268 0269 0270 0271 0272 0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304 0305 0306 0307 0308 0309 0310 0311 0312
0313 0314 0315 0316 0317 0318 0319 0320 0321 0322 0323


























```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 24), y = floor((n − 1) / 24). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.
