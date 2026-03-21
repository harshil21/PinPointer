import os
import subprocess
import numpy as np
import matplotlib.pyplot as plt
from openEMS import openEMS
from CSXCAD import ContinuousStructure
from CSXCAD.SmoothMeshLines import SmoothMeshLines
from openEMS.physical_constants import EPS0, C0

# ═══════════════════════════════════════════════════════════════════════
# 0.  PATHS & MODE
# ═══════════════════════════════════════════════════════════════════════
Sim_Path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'manual_sim')
os.makedirs(Sim_Path, exist_ok=True)
Sim_CSX        = os.path.join(Sim_Path, 'ifa_915.xml')
post_proc_only = False

# ═══════════════════════════════════════════════════════════════════════
# 1.  FREQUENCY
#
#  fc = Gaussian half-bandwidth.  Larger fc → shorter pulse → faster
#  ring-down.  Results are only meaningful up to f0+fc — above that
#  there is no excitation energy, and S11 becomes numerical noise.
#  fc=1e9 gives trustworthy data from 0 to ~1.9 GHz.
# ═══════════════════════════════════════════════════════════════════════
CENTER_FREQUENCY = 915e6
f0 = CENTER_FREQUENCY
fc = 1e9   # half-bandwidth [Hz]

# ═══════════════════════════════════════════════════════════════════════
# 2.  BOARD & MATERIAL
# ═══════════════════════════════════════════════════════════════════════
UNIT             = 1e-3
BOARD_WIDTH      = 50.0    # X [mm]
BOARD_LENGTH     = 65.3    # Y [mm]
BOARD_THICKNESS  = 1.6     # Z [mm]
SUBSTRATE_EPSR   = 4.6
SUBSTRATE_LOSS_TAN = 0.015
SUBSTRATE_KAPPA  = SUBSTRATE_LOSS_TAN * 2*np.pi*CENTER_FREQUENCY * EPS0 * SUBSTRATE_EPSR
KEEPOUT_ZONE     = 10.32   # Y clearance at board top edge [mm]

# ═══════════════════════════════════════════════════════════════════════
# 3.  IFA FIXED DIMENSIONS
# ═══════════════════════════════════════════════════════════════════════
IFA_FEED_WIDTH           = 1.0   # feed element width in X [mm]
IFA_H                    = 8.0   # height of short/feed stubs in Y [mm]
IFA_SHORT_CIRCUIT_WIDTH  = 2.0   # short circuit stub width in X [mm]
IFA_FEED_POINT_RELATIVE  = 3.0   # X offset of feed element from short stub left edge [mm]
W = 1.0                          # radiating element trace width [mm]

# ═══════════════════════════════════════════════════════════════════════
# 4.  MEANDER PARAMETERS  ← PRIMARY TUNING KNOBS
#
#  arm electrical length ≈ IFA_INITIAL_HORIZONTAL
#                         + (N_MEANDERS - 1) × (2×V_GAP + 2×H_GAP)
#                         +                    (2×V_GAP + H_GAP)
#  Target: ~53 mm for 915 MHz (velocity factor ≈ 0.65 on FR4)
#
#  Starting values (arm ≈ 53.5 mm → ~911 MHz):
#    N=4, H_init=3, V_gap=5, H_gap=1.5
#
#  Tuning:
#    f_res too HIGH → INCREASE N_MEANDERS or IFA_INITIAL_HORIZONTAL
#    f_res too LOW  → DECREASE V_GAP or H_GAP
#    Re(Zin) wrong  → adjust IFA_FEED_POINT_RELATIVE (further = higher R)
# ═══════════════════════════════════════════════════════════════════════
N_MEANDERS             = 5     # number of meander U-turns  ← tune this
IFA_INITIAL_HORIZONTAL = 9.0   # [mm] straight segment before first meander ← tune this
IFA_MEANDER_V_GAP      = 4.0   # [mm] clear vertical gap within each meander
IFA_MEANDER_H_GAP      = 2.5   # [mm] horizontal gap between meander turns

# Estimated arm electrical length (centerline trace length)
est_length = (IFA_INITIAL_HORIZONTAL
              + (N_MEANDERS - 1) * (2*IFA_MEANDER_V_GAP + 2*IFA_MEANDER_H_GAP)
              +                    (2*IFA_MEANDER_V_GAP +   IFA_MEANDER_H_GAP))
lam4_free = C0 / (4 * CENTER_FREQUENCY) / UNIT
print("=" * 56)
print(f"  Estimated arm length : {est_length:.1f} mm")
print(f"  λ/4 free space       : {lam4_free:.1f} mm")
print(f"  Velocity factor est. : {est_length/lam4_free:.2f}  (expect 0.60–0.75 on FR4)")
print("=" * 56)

# ═══════════════════════════════════════════════════════════════════════
# 5.  FDTD ENGINE
# ═══════════════════════════════════════════════════════════════════════
sim_box_size = np.array([200, 200, 200])  # mm

FDTD = openEMS(NrTS=90000, MaxTime=120, CoordSystem=0)
FDTD.SetGaussExcite(f0, fc)
# MUR - Simple absorbing boundary condition
FDTD.SetBoundaryCond(['MUR', 'MUR', 'MUR', 'MUR', 'MUR', 'MUR'])

# Setup CSXCAD geometry & mesh
CSX = ContinuousStructure()
FDTD.SetCSX(CSX)
mesh = CSX.GetGrid()
mesh.SetDeltaUnit(UNIT)
mesh_res = C0 / (f0 + fc) / UNIT / 20   # λ/20 at highest excited freq

# initialize the mesh with the "air-box" dimensions
# We center the box at the origin, so we go from -box_size/2 to +box_size/2 in each dimension
mesh.AddLine('x', [-sim_box_size[0] / 2, sim_box_size[0] / 2])
mesh.AddLine('y', [-sim_box_size[1] / 2, sim_box_size[1] / 2])
mesh.AddLine('z', [-sim_box_size[2] / 2, sim_box_size[2] / 2])

# ═══════════════════════════════════════════════════════════════════════
# 6.  SUBSTRATE
# ═══════════════════════════════════════════════════════════════════════
substrate = CSX.AddMaterial('substrate', epsilon=SUBSTRATE_EPSR, kappa=SUBSTRATE_KAPPA)
substrate.AddBox([-BOARD_WIDTH/2, -BOARD_LENGTH/2, 0],
                 [ BOARD_WIDTH/2,  BOARD_LENGTH/2, BOARD_THICKNESS], priority=1)
# Discretise substrate thickness (6 cells = 0.32 mm each)
mesh.AddLine('z', np.linspace(0, BOARD_THICKNESS, 6))

# ═══════════════════════════════════════════════════════════════════════
# 7.  GROUND PLANE  (top copper, below keepout zone)
# ═══════════════════════════════════════════════════════════════════════
groundplane = CSX.AddMetal('groundplane')
groundplane.AddBox(
    [-BOARD_WIDTH/2, -BOARD_LENGTH/2,              BOARD_THICKNESS],
    [ BOARD_WIDTH/2,  BOARD_LENGTH/2 - KEEPOUT_ZONE, BOARD_THICKNESS],
    priority=10)

# ═══════════════════════════════════════════════════════════════════════
# 8.  IFA — FIXED ELEMENTS (feed & short-circuit stubs)
# ═══════════════════════════════════════════════════════════════════════
ifa = CSX.AddMetal('ifa')

# Translation vector: antenna originates at bottom-left of keepout zone
tl = np.array([-BOARD_WIDTH/2, BOARD_LENGTH/2 - KEEPOUT_ZONE, BOARD_THICKNESS])

# Feed element (vertical, from port gap up to arm level)
fe_start = np.array([IFA_FEED_POINT_RELATIVE + 1, 0.5, 0]) + tl
fe_stop  = fe_start + np.array([IFA_FEED_WIDTH, IFA_H, 0])
ifa.AddBox(fe_start.tolist(), fe_stop.tolist(), priority=10)

# Short-circuit stub (vertical, galvanic GND connection)
sc_start = np.array([IFA_FEED_POINT_RELATIVE, 0, 0]) + tl
sc_stop  = sc_start + np.array([-IFA_SHORT_CIRCUIT_WIDTH, IFA_H + 0.5, 0])
ifa.AddBox(sc_start.tolist(), sc_stop.tolist(), priority=10)

# ═══════════════════════════════════════════════════════════════════════
# 9.  IFA — MEANDERED RADIATING ARM
#
#  Geometry (Y direction, looking at board top edge):
#
#   arm_y+W ─── ─────────────┐     ┌──────┐     ┌──────┐     ┌──
#   arm_y   ─── initial ─────┘     │      │     │      │     │  ← open end
#                                   │ down │     │ down │
#   bot_y+W ─────────────────────── │      │─────│      │─────
#   bot_y   ─────────────────────── └──────┘     └──────┘
#               [arm_x0]            ← meanders progress in +X →
#
#  Each meander cycle:
#    down-turn vertical (arm_y+W → bot_y, width W)
#    bottom horizontal connector (length H_gap)
#    up-turn vertical (bot_y → arm_y+W, width W)
#    top horizontal connector (length H_gap, omitted after last meander)
# ═══════════════════════════════════════════════════════════════════════

arm_x0 = tl[0] + IFA_FEED_POINT_RELATIVE - IFA_SHORT_CIRCUIT_WIDTH  # X start of arm
arm_y  = tl[1] + IFA_H + 0.5     # Y bottom edge of arm trace
bot_y  = arm_y - IFA_MEANDER_V_GAP   # Y bottom edge of lower meander trace
gnd_y  = tl[1]                    # Y edge of GND plane (keepout boundary)

# Safety check: meander must not overlap the GND plane
clearance = bot_y - gnd_y
assert clearance > W, (
    f"Meander bottom ({bot_y:.2f} mm) is only {clearance:.2f} mm from GND boundary "
    f"({gnd_y:.2f} mm). Reduce IFA_MEANDER_V_GAP or IFA_H.")

# X limit check (arm must stay within board)
x_per_meander  = 2*W + IFA_MEANDER_H_GAP   # X consumed per meander (down+gap+up)
x_top_connectors = (N_MEANDERS - 1) * IFA_MEANDER_H_GAP
arm_x_end = arm_x0 + IFA_INITIAL_HORIZONTAL + N_MEANDERS * x_per_meander + x_top_connectors
assert arm_x_end < BOARD_WIDTH/2 - 1, (
    f"Arm X end ({arm_x_end:.1f} mm) exits board ({BOARD_WIDTH/2:.1f} mm). "
    f"Reduce N_MEANDERS or H_GAP.")

# ── Collect all metal edge coordinates for fine meshing ───────────────
all_ex = []   # X edge positions of meander boxes
all_ey = [arm_y, arm_y + W, bot_y, bot_y + W]   # fixed Y levels

def add_h(x0, x1, y0):
    """Horizontal trace: y in [y0, y0+W], x in [x0, x1]."""
    ifa.AddBox([x0, y0, BOARD_THICKNESS], [x1, y0 + W, BOARD_THICKNESS], priority=10)
    all_ex.extend([x0, x1])
    all_ey.extend([y0, y0 + W])

def add_v(x0, y_lo, y_hi):
    """Vertical trace: x in [x0, x0+W], y in [y_lo, y_hi]."""
    ifa.AddBox([x0, y_lo, BOARD_THICKNESS], [x0 + W, y_hi, BOARD_THICKNESS], priority=10)
    all_ex.extend([x0, x0 + W])
    all_ey.extend([y_lo, y_hi])

# Track current X position (left edge of next element)
cur_x = arm_x0

# Initial horizontal segment
add_h(cur_x, cur_x + IFA_INITIAL_HORIZONTAL, arm_y)
cur_x += IFA_INITIAL_HORIZONTAL

# Meander loops
for i in range(N_MEANDERS):
    # 1. Down-turn: full-height vertical (spans arm_y+W down to bot_y)
    add_v(cur_x, bot_y, arm_y + W)
    cur_x += W

    # 2. Bottom horizontal connector
    add_h(cur_x, cur_x + IFA_MEANDER_H_GAP, bot_y)
    cur_x += IFA_MEANDER_H_GAP

    # 3. Up-turn: full-height vertical (spans bot_y up to arm_y+W)
    add_v(cur_x, bot_y, arm_y + W)
    cur_x += W

    # 4. Top horizontal connector (between meanders, omit after last)
    if i < N_MEANDERS - 1:
        add_h(cur_x, cur_x + IFA_MEANDER_H_GAP, arm_y)
        cur_x += IFA_MEANDER_H_GAP
    # After the last meander, the arm is open-ended at the up-turn's top-right corner

print(f"  Arm X span : [{arm_x0:.1f}, {cur_x+W:.1f}] mm  "
      f"(board: [{-BOARD_WIDTH/2:.1f}, {BOARD_WIDTH/2:.1f}])")
print(f"  Arm Y range: [{bot_y:.1f}, {arm_y+W:.1f}] mm  "
      f"(GND boundary at {gnd_y:.1f}  clearance={clearance:.1f} mm)")

# ═══════════════════════════════════════════════════════════════════════
# 10.  LUMPED PORT  (50 Ω, Y direction, below feed element)
# ═══════════════════════════════════════════════════════════════════════
port_start = np.array([IFA_FEED_POINT_RELATIVE + 1, 0, 0]) + tl
port_stop  = port_start + np.array([IFA_FEED_WIDTH, 0.5, 0])
port = FDTD.AddLumpedPort(port_nr=1, R=50,
                          start=port_start.tolist(), stop=port_stop.tolist(),
                          p_dir='y', excite=True, priority=5)

# ═══════════════════════════════════════════════════════════════════════
# 11.  MESH
#
#  Strategy:
#   1. Collect every metal edge from stubs + meander boxes → fine 0.5 mm mesh
#   2. mesh.SmoothMeshLines fills remaining gaps at mesh_res (coarser)
#
#  SmoothMeshLines(pts, max_res): standalone function from CSXCAD that
#  generates a smooth array between key points with ≤ max_res spacing.
#  mesh.SmoothMeshLines(dir, max_res, ratio): fills the full domain.
# ═══════════════════════════════════════════════════════════════════════

# Fixed edges from feed element and short-circuit stub
stub_edges_x = [
    tl[0] + IFA_FEED_POINT_RELATIVE - IFA_SHORT_CIRCUIT_WIDTH,  # SC stub left
    tl[0] + IFA_FEED_POINT_RELATIVE,                             # SC stub right
    tl[0] + IFA_FEED_POINT_RELATIVE + 1,                         # feed element left
    tl[0] + IFA_FEED_POINT_RELATIVE + 1 + IFA_FEED_WIDTH,        # feed element right
]
stub_edges_y = [
    tl[1],                   # GND boundary / stub base
    tl[1] + 0.5,             # port gap top / feed base
    tl[1] + IFA_H + 0.5,     # stub top = arm bottom
    tl[1] + IFA_H + W + 0.5, # arm top
]

# Combine stub + meander edges, deduplicate, sort
combined_x = sorted(set(stub_edges_x + all_ex))
combined_y = sorted(set(stub_edges_y + all_ey))

# Fine lines at all metal edges (0.5 mm max spacing between critical points)
mesh.AddLine('x', SmoothMeshLines(combined_x, 0.5))
mesh.AddLine('y', SmoothMeshLines(combined_y, 0.5))

# Fill the rest of the domain (air box) at coarser mesh_res
mesh.SmoothMeshLines('all', mesh_res, 1.4)

nf2ff = FDTD.CreateNF2FFBox()

nx = len(mesh.GetLines('x'))
ny = len(mesh.GetLines('y'))
nz = len(mesh.GetLines('z'))
print(f"  Mesh lines  : X={nx}  Y={ny}  Z={nz}")
print(f"  Total cells : ~{(nx-1)*(ny-1)*(nz-1):,}  (target: 40k–200k)")
print(f"  mesh_res    : {mesh_res:.1f} mm  (λ/20 @ {(f0+fc)/1e6:.0f} MHz)")

# ═══════════════════════════════════════════════════════════════════════
# 12.  GEOMETRY CHECK
# ═══════════════════════════════════════════════════════════════════════
CSX.Write2XML(Sim_CSX)   # Sim_CSX already contains the full path

try:
    subprocess.Popen(['AppCSXCAD', Sim_CSX]).wait()
except FileNotFoundError:
    print("  AppCSXCAD not found — skipping.")

input("\nGeometry correct? Press [ENTER] to start FDTD. Ctrl+C to abort.\n")

# ═══════════════════════════════════════════════════════════════════════
# 13.  RUN FDTD
# ═══════════════════════════════════════════════════════════════════════
if not post_proc_only:
    print(f"Running FDTD in: {Sim_Path}")
    FDTD.Run(Sim_Path, cleanup=True)

# ═══════════════════════════════════════════════════════════════════════
# 14.  POST-PROCESSING
#
#  Plot only up to f0+fc — above that, S11 and Zin are numerical noise
#  (no excitation energy → dividing near-zero by near-zero).
# ═══════════════════════════════════════════════════════════════════════
print("\nCalculating port quantities...")
PLOT_FMIN = 300e6
PLOT_FMAX = f0 + fc    # do not plot beyond excitation bandwidth
f = np.linspace(PLOT_FMIN, PLOT_FMAX, 501)
port.CalcPort(Sim_Path, f)

s11    = port.uf_ref / port.uf_inc
s11_dB = 20.0 * np.log10(np.abs(s11) + 1e-30)
Zin    = port.uf_tot / port.if_tot
P_in   = 0.5 * np.real(port.uf_tot * np.conj(port.if_tot))

imz = np.imag(Zin)
rez = np.real(Zin)

idx_s11 = np.argmin(s11_dB)
f_s11   = f[idx_s11]
s11_min = s11_dB[idx_s11]
R_s11   = rez[idx_s11]
X_s11   = imz[idx_s11]

print(f"\n{'═'*54}")
print(f"  S11 minimum : {f_s11/1e6:.0f} MHz,  {s11_min:.1f} dB")
print(f"  Zin at min  : {R_s11:.0f} + j{X_s11:.0f} Ω")
df = (f_s11 - CENTER_FREQUENCY)/1e6
print(f"  Δf from 915 : {df:+.0f} MHz")
if abs(df) > 10:
    mm = abs(df) / 9.0
    verb = "DECREASE" if df < 0 else "INCREASE"
    print(f"  → {verb} N_MEANDERS or IFA_INITIAL_HORIZONTAL by ≈{mm:.0f} mm equivalent")
print(f"{'═'*54}")

fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)

ax1.plot(f/1e6, s11_dB, 'royalblue', lw=2, label='S11')
ax1.axvline(915, color='crimson', ls='--', lw=1.5, label='915 MHz target')
ax1.axhline(-10, color='gray',  ls=':', lw=1.0, label='−10 dB')
ax1.axhline(-15, color='green', ls=':', lw=1.0, label='−15 dB goal')
ax1.scatter(f_s11/1e6, s11_min, color='crimson', zorder=5,
            label=f'S11 min: {f_s11/1e6:.0f} MHz, {s11_min:.1f} dB')
ax1.set_xlabel('Frequency (MHz)')
ax1.set_ylabel('S11 (dB)')
ax1.set_title(f'S11  —  Meandered IFA  (N={N_MEANDERS}, H_init={IFA_INITIAL_HORIZONTAL} mm, '
              f'V_gap={IFA_MEANDER_V_GAP} mm)')
ax1.set_xlim([PLOT_FMIN/1e6, PLOT_FMAX/1e6])
ax1.set_ylim([-40, 5])
ax1.grid(True, alpha=0.35)
ax1.legend(fontsize=8)

ax2.plot(f/1e6, rez, 'k-',  lw=2, label='Re{Zin}')
ax2.plot(f/1e6, imz, 'r--', lw=2, label='Im{Zin}')
ax2.axvline(915, color='royalblue', ls='--', lw=1.5, label='915 MHz')
ax2.axhline(50,  color='green',     ls=':',  lw=1.2, label='50 Ω')
ax2.axhline(0,   color='gray',      ls='-',  lw=0.8)
ax2.set_xlabel('Frequency (MHz)')
ax2.set_ylabel('Impedance (Ω)')
ax2.set_title('Zin  —  S11 minimum ≈ where Im(Zin) crosses 0 and Re(Zin) ≈ 50 Ω')
ax2.set_xlim([PLOT_FMIN/1e6, PLOT_FMAX/1e6])
ax2.set_ylim([-200, 300])
ax2.grid(True, alpha=0.35)
ax2.legend(fontsize=8)

plt.savefig(os.path.join(Sim_Path, 's11_impedance.png'), dpi=150, bbox_inches='tight')
plt.show()