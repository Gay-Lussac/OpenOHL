# Steel pole shafts by ASCE/SEI 48-19

Implemented in `crates/ohl-structures-fe/src/asce48.rs` (`polygon_allowable`,
`round_allowables`, `Shaft`, `usage`), used by the pole check
(`crates/ohl-structures-fe/src/pole.rs`: `Pole::by_code`, `forces_at`,
`check_pole_factored`). Built, in active development (not approved yet);
pre-alpha: results must be verified by a qualified engineer.

**Source.** ASCE/SEI 48-19, *Design of Steel Transmission Pole
Structures*, American Society of Civil Engineers (2020), Section 5.2
(members). Clause and equation numbers below are the standard's. ASCE
10-97 (the tower check, [its page](asce10.md)) covers latticed structures
only and has no rule for tubes; ASCE/SEI 48 is its companion for poles
(owner, 2026-11-03).

**Where it is used.** A pole (`.ohlp`, or `[pole]` in a structure file)
of material `steel`, hollow (a `wall`), with its `yield` stress and **no
capacity given** (no `[[capacity]]` table, `moment_capacity` or
`fibre_stress`): its shaft is checked by this page at the pole check's 41
heights. A given capacity keeps the moment check of the
[structure analysis](structures-fe.md#pole-check) page, as before. A
`[pole]` naming a `section` takes the yield stress of that section's
material when it gives none itself.

## Forces at a height

At height \\( h \\) above the ground line, from the factored loads above
and the wind on the shaft above:

| Force | |
|---|---|
| bending moment \\( M \\) | as the pole check: \\( \lvert \sum (\mathbf r_i - h\hat{\mathbf z}) \times \mathbf F_i + \text{wind} \rvert \\), times the P-Delta amplification |
| axial force \\( P \\) (compression positive) | \\( -\sum F_{z,i} \\) of the loads above, plus the shaft's own weight above \\( \int_h^H \rho\,g\,A(z)\,dz \\) times the case's vertical factor |
| shear \\( V \\) | \\( \lvert \sum (F_{x,i}, F_{y,i}) + \int_h^H p\,c_d\,D(z)\,dz\;\hat{\mathbf w} \rvert \\) |
| torsion \\( T \\) | \\( \sum (x_i F_{y,i} - y_i F_{x,i}) \\) of the loads above |

## Section properties

A regular polygon of \\( n \\) sides, \\( D \\) across flats (outside),
wall \\( t \\), or a round tube of outside diameter \\( D \\): area \\( A \\)
and second moment \\( I \\) of the outside less the inside shape (a
regular polygon's \\( I \\) is the same about every axis); \\( c \\) the
farthest fibre, a polygon's corner \\( D/(2\cos(\pi/n)) \\), a round tube's
\\( D/2 \\).

**Flat width** \\( w \\) (5.2.3.2.1): the side of the inside face,
\\( (D - 2t)\tan(\pi/n) \\), less the bends: \\( 2 r \tan(\pi/n) \\) with the
inside bend radius \\( r \\) (`bend_radius`), "unless it exceeds 4t, in which
case it shall be taken equal to 4t". **Conservative:** without a bend
radius the corners are taken as sharp — the widest flat, the lowest
permitted stress.

## Permitted stresses

With \\( x = \dfrac{w}{t}\sqrt{F_y/E} \\), \\( E \\) = 200 GPa as the
standard defines it (5.2.1.2), and the bend angle \\( 360°/n \\):

| Bend angle (sides) | Axial stress | \\( F_a = F_y \\) for | \\( F_a = k F_y (1 - s\,x) \\) for | k, s | Eqs. |
|---|---|---|---|---|---|
| ≥ 45° (up to 8) | ≤ 6.9 MPa | \\( x \le 1.53 \\) | \\( 1.53 < x \le 2.06 \\) | 1.42, 0.194 | 5.2-6, 5.2-7 |
| ≥ 45° (up to 8) | > 6.9 MPa | \\( x \le 1.41 \\) | \\( 1.41 < x \le 2.20 \\) | 1.45, 0.220 | 5.2-8, 5.2-9 |
| 30° to 45° (9 to 12) | — | \\( x \le 1.41 \\) | \\( 1.41 < x \le 2.20 \\) | 1.45, 0.220 | 5.2-8, 5.2-9 |
| 22.5° to 30° (13 to 16) | — | \\( x \le 1.26 \\) | \\( 1.26 < x \le 2.42 \\) | 1.42, 0.233 | 5.2-10, 5.2-11 |
| < 22.5° (more than 16), round | — | see below | | | 5.2-12 to 5.2-16 |

(Table 5-1.) Beyond the upper limit the standard permits the section only
"if local buckling stability is demonstrated by an adequate program of
tests" (5.2.3.2): the check says so and gives no usage below infinity.

**Round tubes and polygons of more than 16 sides** (5.2.3.2.4), with
\\( D_o/t \\) (outside diameter over wall) and \\( \rho = E/F_y \\):

\\[ F_a = \begin{cases} F_y & D_o/t \le 0.131\rho \\ 0.75F_y + 0.0328\,E/(D_o/t) & \le 0.414\rho \end{cases} \quad (5.2\text{-}13, 14) \\]

\\[ F_b = \begin{cases} F_y & D_o/t \le 0.207\rho \\ 0.70F_y + 0.0621\,E/(D_o/t) & \le 0.414\rho \end{cases} \quad (5.2\text{-}15, 16) \\]

and \\( f_a/F_a + f_b/F_b \le 1 \\) (5.2-12). Above \\( 0.414\rho \\): by tests
only, said.

## The check

At each height, with \\( f_a = P/A \\), \\( f_b = Mc/I \\) and the shear
stress \\( \tau \\):

- **combined stress** (5.2.6, Eqs. 5.2-20, 5.2-21):
  \\( \sqrt{(\pm f_a + f_b)^2 + 3\tau^2} \\) — on the compression side
  (\\( f_a \\) of compression) against \\( F_a \\) (polygon) or \\( F_b \\) (round),
  on the tension side (\\( f_a \\) of tension) against \\( F_y \\); bending is
  thereby also within 5.2.5 (Eqs. 5.2-18, 5.2-19);
- **shear** (5.2.4, Eq. 5.2-17): \\( \tau \le 0.58 F_y \\);
- **round** (5.2.3.2.4): \\( f_a/F_a + f_b/F_b \le 1 \\) (5.2-12).

The usage is the largest ratio, the permitted stresses times the load
case's strength factor \\( \phi \\); what governs names it: `ASCE/SEI
48-19 at 0.0 m: compression side 214 MPa of F_a 317 MPa (Eq. 5.2-9, w/t
40.0; 5.2-20) × 1; moment 412.0 kN m (P-Delta × 1.032)`. The largest
over the heights is the pole's.

**Shear stress.** \\( VQ/(Ib) \\) with \\( b = 2t \\) is taken as \\( 2V/A \\),
the thin-walled tube's largest (at the neutral axis); the torsion's
\\( Tc/J \\) as Bredt's \\( T/(2A_m t) \\), \\( A_m \\) the area inside the
wall's midline (exact for a thin closed section). **Conservative:** the
two are added at one point (the bending's largest stress and the shear's
largest are not at the same point of the section).

## Not covered

Unsafe where one of them governs; the standard's clause is the reference.

- 5.2.2 tension on the net section (\\( 0.83 F_u \\)), where the shaft has
  holes;
- 5.2.3.1 truss members (a pole is a beam member here), rectangular and
  elliptical shafts (5.2.3.2.2, 5.2.3.2.3), shafts of several plies;
- slip joints, base plates, anchor bolts, arms and their connections,
  guys (5.3), deflection limits and the foundation;
- the yield stress by thickness (one value per pole).

## Validation

- Unit tests (`asce48::tests`): the permitted stresses of Table 5-1 and
  Eqs. 5.2-13 to 5.2-16 by hand, the range limits, the flat width with
  and without a bend radius; (`pole_extension_tests::a_steel_shaft_by_asce48`)
  a round shaft's combined stress at the ground line by hand, a given
  capacity keeping the old check.
