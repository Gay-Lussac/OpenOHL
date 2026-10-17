# Structure analysis: frames, trusses, guys and poles

Implemented in `crates/ols-structures-fe`: a linear-elastic 3D finite-element model of a structure (lattice tower, frame, guyed mast, pole) under static loads, first order or with geometric non-linearity (P-Delta), and the member and pole checks built on it. It is a *base*: the formulation is standard matrix structural analysis; design-code specifics (column curves, connection checks, wind on the structure) are left to the data or to later work.

## Model and axes

Nodes are given in the **structure's axes**: x to the right of the line looking ahead, y ahead, z up, origin at the ground centre (the axes of the structure types, `.olss`). Each node has six degrees of freedom \\( (u_x, u_y, u_z, \theta_x, \theta_y, \theta_z) \\); rotations exist only at nodes with a frame member, fixed degrees of freedom are removed, springs add to the diagonal.

Each member has **local axes**: \\( x \\) from node *i* to node *j*; a reference direction \\( \mathbf r \\) in the local x–y plane (default: global z, or global x for a vertical member) gives \\( \mathbf z = \mathbf x \times \mathbf r / |\cdot| \\), \\( \mathbf y = \mathbf z \times \mathbf x \\); a roll angle turns y and z about x. \\( I_z \\) is the bending in the local x–y plane, \\( I_y \\) in the x–z plane. The element matrices are transformed with \\( \mathbf T = \operatorname{diag}(\mathbf R, \mathbf R, \mathbf R, \mathbf R) \\), \\( \mathbf K = \mathbf T^T \mathbf k \mathbf T \\).

## Elements

- **Truss** (axial only): \\( \mathbf k = \frac{EA}{L}\\,\mathbf e\mathbf e^T \\) in the 3×3 blocks \\( \begin{bmatrix} \mathbf k & -\mathbf k \\\\ -\mathbf k & \mathbf k \end{bmatrix} \\).
- **Cable** (guys): a truss in tension only, with an initial force \\( T_0 \\) (pretension): \\( N = T_0 + \frac{EA}{L}\\,\mathbf e\cdot(\mathbf u_j - \mathbf u_i) \\). The pretension acts as a pair of nodal forces pulling the ends together; the force after the structure deflects differs from \\( T_0 \\) by the structure's give. A cable whose force would fall below zero is **slack**: it is removed and the solve repeated until the set of slack cables does not change.
- **Frame**: the 12×12 elastic stiffness of the 3D Euler–Bernoulli beam (axial \\( EA/L \\), torsion \\( GJ/L \\), bending \\( 12EI/L^3, 6EI/L^2, 4EI/L, 2EI/L \\) in both planes) — Przemieniecki, *Theory of Matrix Structural Analysis* (1968), ch. 5. Shear deformation is neglected.

**Self-weight**: \\( w = \rho A g \\) per metre, times the load case's vertical factor. On frames as consistent nodal loads (\\( wL/2 \\) and \\( wL^2/12 \\) end moments, in local axes), on trusses and cables half at each end. The reported member end forces of frames subtract the fixed-end part, so they are the true end forces.

## Second order (P-Delta)

With `nonlinear = true` (the default), the tangent stiffness includes the **geometric stiffness** of each member's axial force \\( N \\) (tension positive):

- truss and cable: \\( \frac{N}{L}(\mathbf I - \mathbf e\mathbf e^T) \\) in the 3×3 blocks;
- frame: the consistent geometric stiffness of the cubic beam (terms \\( \frac{N}{L}\\{\tfrac65, \tfrac{L}{10}, \tfrac{2L^2}{15}, -\tfrac{L^2}{30}\\} \\) in both bending planes) — McGuire, Gallagher & Ziemian, *Matrix Structural Analysis* (2nd ed., 2000), ch. 9. The torsion–bending coupling terms are left out.

This captures both P-Δ (sway of the member chords) and P-δ (bowing within a frame member, refined by more elements). Displacements remain small (no corotational update of the geometry); the analysis is a *second-order elastic* analysis in the sense of McGuire et al., ch. 9.

**Newton iterations**: the out-of-balance forces \\( \mathbf r = \mathbf F - \mathbf K_T(N)\\,\mathbf u \\) are solved with the tangent \\( \mathbf K_T = \mathbf K_e + \mathbf K_g(N) \\) (the change of \\( N \\) with \\( \mathbf u \\) is not in the tangent), \\( \mathbf u \mathrel{+}= \mathbf K_T^{-1}\mathbf r \\), the axial forces and slack cables updated, until \\( |\mathbf r| \le 10^{-9}|\mathbf F| \\) and the slack set is stable. Where the axial forces are statically determinate (a cantilever column) the second iteration is final; otherwise the iterations converge linearly with the ratio of the loads to the buckling load. A negative pivot in the factorisation means the tangent stiffness is no longer positive definite: the loads are above an **elastic buckling** load, reported as such.

**Exit clauses** (`ols_core::budget`): 200 iterations and 30 s by default; when one stops the solve, the error says which and how large the out-of-balance forces still were.

## Solver

The stiffness is symmetric and banded once the nodes are numbered well: nodes are ordered by **reverse Cuthill–McKee** (George & Liu, *Computer Solution of Large Sparse Positive Definite Systems*, 1981) and the matrix is stored as a **skyline** (profile) and factorised \\( \mathbf L\mathbf D\mathbf L^T \\) in place (active-column method, Bathe, *Finite Element Procedures*, §8.2). The cost is about \\( n\bar b^2/2 \\) for \\( n \\) equations of mean column height \\( \bar b \\): a generated 1000-member lattice tower solves in a few milliseconds (release build). A zero pivot names the node and direction that are free to move (a mechanism).

Reactions are the internal forces at the supported degrees of freedom less the external loads there; springs report their force as a reaction.

## Member usage

Against the section's capacity times the load case's strength factor \\( \phi \\):

| Member | Usage |
|---|---|
| Truss in tension | \\( N / (\phi N_t) \\), \\( N_t \\) = the allowable tension, else \\( A f_y \\) |
| Truss in compression | \\( \lvert N \rvert / (\phi N_c) \\), \\( N_c = \min(\text{allowable or } A f_y,\\; \pi^2 E I_{min} / (kL)^2) \\) |
| Cable | tension as above; a slack cable uses 0 |
| Frame | \\( \lvert N \rvert/(\phi N_{t,c}) + M_y/(\phi M_{y,c}) + M_z/(\phi M_{z,c}) \\), largest end moments, \\( M_c \\) = allowable, else \\( W f_y \\) |

The Euler load with an effective length factor \\( k \\) is a simple stand-in: a design code's column curve (imperfections, residual stresses, slenderness of angle sections, eccentric connections) is not modelled — the user can give the code's allowable compression per section instead. A structure's FE usage is its worst member.

## Member wind

A member whose section comes from the section library has a profile; the wind on it (`Model::member_wind`) follows the cross-flow principle: with the unit wind direction \\( \hat{\mathbf w} \\), the member's axis \\( \hat{\mathbf x} \\) and the wind's component normal to it \\( \mathbf w_n = \hat{\mathbf w} - (\hat{\mathbf w}\cdot\hat{\mathbf x})\hat{\mathbf x} \\) (length \\( \sin\theta \\)), the force on the member is

\\[ \mathbf F = q\, c_d\, d\, L \sin\theta \; \mathbf w_n \qquad (|\mathbf F| = q\, c_d\, d\, L \sin^2\theta), \\]

\\( q \\) the wind pressure (times the load case's transverse factor), \\( c_d \\) the model's `drag` (default 1), \\( d \\) the profile's outline width across \\( \mathbf w_n \\) in the member's local y–z plane, \\( L \\) its length. Half of \\( \mathbf F \\) goes to each joint: statically equivalent for the rest of the structure (a cantilever's base moment is exactly \\( q d L^2/2 \\)), the member's own bending under the uniform load left out. No shielding between members, no solidity ratio, no code shape factors; the type's height-band wind (`[[wind.band]]`) is never put on the model, so the two are not added.

## Pole check

A pole (`.olsp`) is a shaft with a linear taper between the ground-line diameter \\( D_0 \\) and the top diameter \\( D_1 \\) over the height above ground \\( H = \ell - e \\) (length less embedment). At heights \\( h \\) from the ground line to the top (41 sections):

\\[ \mathbf M(h) = \sum_{z_i > h} (\mathbf r_i - h\hat{\mathbf z}) \times \mathbf F_i + \int_h^H p\\,c_d\\,D(z)\\,(z-h)\\,\mathrm dz\\;\hat{\mathbf z} \times \hat{\mathbf w} \\]

with the factored attachment loads \\( \mathbf F_i \\) at the type's attachment points \\( \mathbf r_i \\) (horizontal loads × height, vertical loads × their offset from the axis), the wind pressure \\( p \\) of the weather case times the transverse factor, the drag factor \\( c_d \\) and the wind direction \\( \hat{\mathbf w} \\) (across the line). The bending moment is \\( |(M_x, M_y)| \\); torsion is not checked.

**P-Delta amplification**: when the pole gives its modulus \\( E \\), a frame model of the pole above ground (12 tapered steps plus a node at every load, fixed at the ground line, solid or hollow circular sections, its own weight) is solved first order and second order with the same loads (offsets as moments); the ratio of the ground-line moments multiplies the moment at every height — exact at the ground line, a simplification above.

**Capacity** at height \\( h \\): the table of the pole (linear between heights), else for wood \\( f_b\\,\pi D(h)^3/32 \\) with the user's fibre stress \\( f_b \\), else the ground-line moment capacity. Usage: \\( \max_h M(h)/(\phi\\,M_c(h)) \\) — for a tapered wood pole the critical section can lie above the ground line.

Not included: foundation (embedment) check, shear, deflection limits, guyed poles (use an `.olsfe` model with cables), the wind on crossarms and equipment.

## Validation

`benchmarks/structures-fe/frames.toml`: cantilever tip deflection and rotation (\\( PL^3/3EI \\), \\( PL^2/2EI \\)), triangle truss member forces (method of joints), P-Delta cantilever column at half its buckling load against the exact beam-column (Timoshenko & Gere, *Theory of Elastic Stability*, §1.11), guyed mast with pretension (both guys taut), a simply supported beam under its own weight (\\( 5wL^4/384EI \\)) and a wood pole's ground-line moment with wind on the taper. Unit tests add the slack-guy regime, buckling detection, exit clauses, mechanisms, a 1000-member tower and the example line (`examples/synthetic-line`: type-02 with an invented lattice model, type-05 with an invented steel pole).
