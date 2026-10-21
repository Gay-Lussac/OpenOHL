# Optimum structure spotting

Spotting chooses positions, types and heights of the structures along a route so that the line costs least while every constraint holds. Treated as a shortest-path problem over candidate structure sites, it is solved exactly (on the candidate grid) by **dynamic programming**. This approach goes back to Mitra and Wolfenden (*A computer technique for optimizing the sites and heights of transmission line towers — a dynamic programming approach*, The Computer Journal 10(4), 1968) and has been used, with variations, in most optimum-spotting programs since (e.g. Bellman's principle of optimality, R. Bellman, *Dynamic Programming*, Princeton University Press, 1957).

## The problem

- **Ground profile** \\( g(s) \\): station \\( s \\) along the route's centreline, elevation \\( g \\), linear between the points where the centreline crosses the edges of the terrain triangles.
- **Candidate stations** \\( s_0 < s_1 < \dots < s_{N-1} \\): every `step` metres from the start, plus the **required** stations (start, end, alignment points). Stations inside forbidden zones are dropped. No span may pass over a required station.
- **Candidates** \\( k \\) (a structure type with an extension): cost \\( C_k \\), height of the lowest wire point \\( h_k \\), dead-end or not, allowable spans by line angle. At each station only the candidates that may stand there are kept: dead-ends at the ends; at an alignment point those whose allowable spans reach its angle.
- **A span** from \\( (i, k) \\) to \\( (j, l) \\), of length \\( L = s_j - s_i \\), is feasible when \\( L_{min} \le L \le L_{max} \\) and the wire keeps the clearance \\( c \\).
- **Each structure** \\( (j, l) \\) between spans \\( L_b \\) (back) and \\( L_a \\) (ahead) must keep its **wind span** \\( (L_b + L_a)/2 \\) and **weight span** \\( (L_b - x_b) + x_a \\) — \\( x \\) the horizontal distance of a span's low point from its back structure — within its allowable spans at its line angle (the start has only \\( L_a \\), the end only \\( L_b \\)).
- **Cost**: \\( \sum C_k \\) over the structures \\( + \sum (C_{span} + p\\,\ell) \\) over the spans, with \\( \ell \\) the wire length of a span and \\( p \\) the price per metre of all its wires.

## The wire and the clearance

In the design (maximum sag) condition the wire is a catenary of horizontal tension \\( H \\) and weight \\( w \\) per metre, parameter \\( C = H/w \\), between the wire points \\( z_i = g(s_i) + h_k \\) and \\( z_j = g(s_j) + h_l \\) ([Sag-tension](sagtension.md)). Between two consecutive profile points the ground is a straight line of slope \\( m \\), and the wire minus the ground is convex, so its least value on that piece is where the wire's slope equals \\( m \\):

\\[ x^* = x_{low} + C \operatorname{asinh} m, \\]

clamped to the piece. The smallest clearance of the span is the least of these values — exact, with one evaluation per profile piece. A span is rejected as soon as one piece falls below \\( c \\).

**Conservative:** the wire hangs from the lowest wire point of each candidate (\\( h_k \\)): higher wires of the same structure clear the ground by more. Exact: each wire from its own point.

**Simplification:** one catenary of the given \\( H \\) and \\( w \\) for every span (the ruling-span idea), in the vertical plane over the route's centreline, in still air. Real spans differ in tension with their lengths, the outer phases hang beside the centreline over side slopes, and wind swings the wires; each can lower the clearance (unsafe). Exact: the placement solved and checked as a line ([Sag-tension](sagtension.md), [Terrain and ground clearance](../user-guide/terrain.md)), which is what a spotted route is for next.

**Simplification:** holes in the terrain along the route are bridged in a straight line (said in the notes). A hollow is then filled (safe), a hump in the hole missed (unsafe). Exact: terrain without holes.

## Dynamic programming

The state is \\( (j, l, i) \\): a structure of candidate \\( l \\) at station \\( j \\) whose back span starts at station \\( i \\). Its value \\( V(j, l, i) \\) is the least cost of a chain of structures from the start to it:

\\[ V(j, l, i) = C_l + \min_{k,\\,h} \left[ V(i, k, h) + C_{span} + p\\,\ell(i,k \to j,l) \right] \\]

over the candidates \\( k \\) at \\( i \\) and the back stations \\( h \\) of \\( i \\), for feasible spans only, and where the structure \\( (i, k) \\) keeps its wind span \\( (s_i - s_h + s_j - s_i)/2 \\) and weight span. The answer is the least \\( V(N-1, l, i) \\) whose end structure keeps its own limits; the placement follows back from it.

Keeping the back station in the state makes every wind span exact. The weight span at \\( i \\) also needs the low point of the back span \\( h \to i \\), which depends on the candidate at \\( h \\): the state keeps the low point of its cheapest way in. A placement that is only feasible through a dearer structure at \\( h \\) (whose different height shifts the low point just enough) can therefore be missed. This is rare — extensions of one type move the low point little — and the tests compare the result with an exhaustive search on small hilly profiles.

**Simplification:** that low point. It costs only optimality: the placement found keeps every limit with the low points of its own structures; a cheaper one may exist. Exact: the candidate at the back station in the state too (\\( K \\) times the states).

## Cost

With \\( N \\) stations, \\( K \\) candidates and \\( R \\) stations within the longest span of each other:

- states: \\( N K R \\), memory \\( O(N K R) \\);
- span checks: \\( N K^2 R \\), each walking the profile points of the span;
- transitions: \\( O(N K^2 R^2) \\).

A 5 km route at 20 m steps with four candidates takes about 0.15 s in a debug build; a 10 km route at 10 m steps with 6 candidates and spans up to 500 m is of the order of \\( 10^8 \\) transitions — seconds in a release build. Every state counts as one iteration of the run's budget (`ohl_core::budget`, by default 500 million states or 5 minutes, or cancelled by the user); a stopped run reports why and gives no placement.

## Validation

- Exhaustive search: on six rough profiles (seven inner stations, three candidates with wind, weight and uplift limits), every placement is evaluated and the least cost compared with the dynamic programme's (`ohl-spotting` tests).
- Level ground (`benchmarks/spotting/flat.toml`, reference by hand in `reference_spotting.py`): the longest span keeping the clearance is \\( L_{max} = 2C \operatorname{acosh}(1 + (h - c)/C) \\), the optimum is the fewest spans of the longest grid length.
- A hill under a span: the optimum takes a taller extension (or, without one, an extra structure).
