# Engines Overview

*Prompt*: Please tell me about the indie game development landscape from a technical perspective.

The short version is that the technical landscape has stratified in a way it hadn't a few years ago: there's a clear three-way engine split, a healthy tier of lighter-weight frameworks below it, and a set of unglamorous pipeline problems that end up mattering more than engine choice for most projects.

**The engine tier**

The practical decision is less "which engine is best" and more "how much of the stack do I want to own." Unity, Unreal, and Godot sit at different points on that axis.

Unreal is the answer when your game's differentiator is visual fidelity. Nanite and Lumen genuinely remove optimization work that used to eat months, and MetaHuman handles a problem — believable human characters — that is otherwise brutally expensive. The cost is that it's C++-and-Blueprint heavy, the editor is enormous, and build times will humble you. Epic shipped 5.8 at State of Unreal 2026 with a focus on performance and maturing existing features, and announced Unreal Engine 6 targeting Early Access at the end of 2027 — so UE5 is the stable target for anything you're shipping in the next two years.

Unity remains the pragmatic middle: C#, the deepest asset ecosystem, and by far the best mobile export path. Its reputation took real damage from the 2023 runtime-fee episode, and some of that never came back, but the technology itself is solid. The residual concern is structural rather than technical — you're building on terms a company can revise.

Godot has crossed from "promising" to "production-viable," but the boundary matters. It's genuinely competitive for 2D and stylized 3D, with a fast iteration loop and a tiny editor footprint. Jolt Physics became the default 3D engine in 4.6, and 4.7 landed in June 2026 with HDR output, a rewritten Asset Store, and a rectangular area-light node. What it doesn't have is any equivalent to Nanite, Lumen, or MetaHuman, and that gap isn't closing soon. If you're chasing photorealism, Godot is the wrong tool; if you're making a stylized 2D or 3D game, the zero-royalty, zero-license-risk story is compelling.

**Below the engines**

There's a real and underappreciated tier of frameworks — Bevy (Rust), MonoGame (C#), LÖVE (Lua), raylib (C), and various bespoke setups. The useful heuristic: engines exist mainly to give you an *editor*, and editors exist to manage content. If your game is content-heavy — lots of levels, art, scripted sequences — you want an engine. If your game's core is a simulation or systems problem (a roguelike, a management sim, something procedural), the editor buys you less and you may iterate faster with a framework and your own tooling. Several notable indie hits have gone this route.

**The pipeline problems nobody warns you about**

This is where projects actually stall, and it's mostly not engine-specific:

*Version control* is the most consistently underrated. Git handles code well and binary art assets badly. Git LFS is the usual compromise; Perforce is the industry answer but is heavy for two-person teams. Epic announced a new version control system, Lore, at this year's State of Unreal, which suggests they see the same gap.

*Build and iteration time* compounds. A 90-second iteration loop versus a 20-second one is the difference between tuning game feel a hundred times a day and twenty. This is worth engineering for early.

*The boring infrastructure* — save/load, settings menus, input remapping, localization, controller support, achievements, crash reporting — is a genuinely large fraction of a shippable game and is usually estimated at near zero.

*Console porting* is as much a legal and NDA problem as a technical one. Devkits require platform-holder approval; the SDKs are under NDA; most small teams use a porting house rather than doing it in-house.

**Platform targets**

PC-first via Steam is the default, and handhelds have changed what "PC" means technically — you're now targeting a low-wattage Linux-ish device with a fixed controller layout as a first-class case, not an afterthought. Web export via WASM has gotten genuinely usable in Godot and workable in Unity; it remains impractical in Unreal. Mobile is a different discipline entirely, closer to a live-service and monetization problem than a craft problem.

**The context that shapes all of it**

Technical choices are downstream of a harsh distribution market. SteamDB's release tracker shows Steam's cumulative catalog well past 120,000 titles, with more than 19,000 launching in 2025 alone — and roughly 9,300 of those drew fewer than ten user reviews. The platform now averages around 350 new releases a week.

The engineering implication is real: scope discipline beats technical ambition. The engine that lets you finish is better than the engine that could theoretically render your game more beautifully. Most postmortems that blame an engine are actually describing a scope problem.

One caveat on research — engine market-share figures get quoted constantly and the sources disagree wildly, sometimes by a factor of five for the same metric. Treat any specific percentage you see with suspicion unless it traces back to SteamDB, GameDiscoverCo, or the GDC State of the Industry survey.
