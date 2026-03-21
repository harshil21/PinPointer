import os
import subprocess
import numpy as np
import matplotlib.pyplot as plt
from openEMS import openEMS
from CSXCAD import ContinuousStructure
from CSXCAD.SmoothMeshLines import SmoothMeshLines
from openEMS.physical_constants import EPS0, C0

# ═══════════════════════════════════════════════════════════════════════
# PARAMETERS
# ═══════════════════════════════════════════════════════════════════════

CENTER_FREQUENCY  = 915e6
FC_HALF_BANDWIDTH = 100e6       # valid results up to CENTER + this

BOARD_WIDTH       = 42.12     # X [mm]
BOARD_LENGTH      = 65.54    # Y [mm]
BOARD_THICKNESS   = 1.6       # Z [mm]
SUBSTRATE_EPSR    = 4.6
SUBSTRATE_LOSS_TANGENT = 0.015

KEEPOUT_WIDTH  = 13.18   # left-edge strip with no GND copper [mm]
TOP_MARGIN     = 2.0    # gap from board top to SC stub top [mm]
BOTTOM_MARGIN  = 1.0    # minimum clearance from board bottom to arm end [mm]

SC_STUB_LENGTH   = 12.0  # SC stub X length; arm spine sits at GND_X - SC_STUB_LENGTH [mm]
SC_TRACE_WIDTH   = 1.0  # SC stub Y height [mm]
FEED_TRACE_WIDTH = 0.5  # feed stub Y height [mm]
FEED_SEPARATION  = 0.3  # Y gap from SC stub bottom to feed stub top [mm]  ← IMPEDANCE TUNE
                         #   Re(Zin) < 50 Ω → decrease;  Re(Zin) > 50 Ω → increase
ARM_TRACE_WIDTH  = 1.0  # arm spine and meander trace width [mm]
PORT_WIDTH       = 0.5  # lumped port gap width [mm] — do not change

N_MEANDERS    = 2       # number of U-turns                         ← PRIMARY FREQUENCY TUNE
INIT_LENGTH   = 10.0    # spine length before first U [mm]           ← fine frequency tune
MEANDER_WIDTH = 7.0     # X span of each U from arm spine left edge [mm]  (< SC_STUB_LENGTH)
MEANDER_V_GAP = 4.0    # open vertical gap inside each U [mm]       ← medium frequency tune
MEANDER_H_GAP = 4.0     # spine segment between consecutive Us [mm]  (only matters if N > 1)
TAIL_LENGTH   = 16.4     # spine after last U [mm]                    ← fine frequency tune

# ═══════════════════════════════════════════════════════════════════════
# DERIVED POSITIONS  (do not edit)
# ═══════════════════════════════════════════════════════════════════════

SUBSTRATE_KAPPA = SUBSTRATE_LOSS_TANGENT * 2*np.pi*CENTER_FREQUENCY * EPS0 * SUBSTRATE_EPSR

GND_X           = -BOARD_WIDTH/2 + KEEPOUT_WIDTH
PORT_LEFT       = GND_X - PORT_WIDTH
ARM_SPINE_X     = GND_X - SC_STUB_LENGTH
ARM_SPINE_RIGHT = ARM_SPINE_X + ARM_TRACE_WIDTH
MEANDER_RIGHT_X = ARM_SPINE_X + MEANDER_WIDTH

SC_STUB_TOP   = BOARD_LENGTH/2 - TOP_MARGIN
SC_STUB_BOT   = SC_STUB_TOP - SC_TRACE_WIDTH
ARM_TOP       = SC_STUB_BOT
FEED_STUB_TOP = ARM_TOP - FEED_SEPARATION
FEED_STUB_BOT = FEED_STUB_TOP - FEED_TRACE_WIDTH

# Estimated arm electrical path length [mm]
_horiz = MEANDER_WIDTH - ARM_TRACE_WIDTH   # unique horizontal per bar
EST_ARM_MM = (INIT_LENGTH
              + N_MEANDERS * (2*_horiz + MEANDER_V_GAP)
              + max(0, N_MEANDERS - 1) * MEANDER_H_GAP
              + TAIL_LENGTH)
LAM4_MM = C0 / (4*CENTER_FREQUENCY) / 1e-3

print(f"  Estimated arm : {EST_ARM_MM:.1f} mm  |  λ/4 free-space : {LAM4_MM:.1f} mm  "
      f"|  vf ≈ {EST_ARM_MM/LAM4_MM:.2f}  (FR4 IFA: 0.60–0.75)")

assert MEANDER_RIGHT_X < GND_X, \
    f"Meander right {MEANDER_RIGHT_X:.1f} overlaps GND {GND_X:.1f} — reduce MEANDER_WIDTH"
assert FEED_SEPARATION + FEED_TRACE_WIDTH <= INIT_LENGTH, \
    f"Feed stub overlaps first meander — increase INIT_LENGTH or decrease FEED_SEPARATION"

# ═══════════════════════════════════════════════════════════════════════
# FDTD
# ═══════════════════════════════════════════════════════════════════════

Sim_Path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'manual_sim')
os.makedirs(Sim_Path, exist_ok=True)
Sim_CSX = os.path.join(Sim_Path, 'ifa_915_vert.xml')
post_proc_only = False

FDTD = openEMS(NrTS=120_000, MaxTime=150, CoordSystem=0, EndCriteria=1e-4)
FDTD.SetGaussExcite(CENTER_FREQUENCY, FC_HALF_BANDWIDTH)
FDTD.SetBoundaryCond(['MUR'] * 6)

CSX  = ContinuousStructure()
FDTD.SetCSX(CSX)
mesh = CSX.GetGrid()
mesh.SetDeltaUnit(1e-3)
MESH_RESOLUTION = C0 / (CENTER_FREQUENCY + FC_HALF_BANDWIDTH) / 1e-3 / 20

mesh.AddLine('x', [-100, 100])
mesh.AddLine('y', [-100, 100])
mesh.AddLine('z', [-100, 100])

# ═══════════════════════════════════════════════════════════════════════
# SUBSTRATE
# ═══════════════════════════════════════════════════════════════════════

substrate = CSX.AddMaterial('substrate', epsilon=SUBSTRATE_EPSR, kappa=SUBSTRATE_KAPPA)
substrate.AddBox([-BOARD_WIDTH/2, -BOARD_LENGTH/2, 0],
                 [ BOARD_WIDTH/2,  BOARD_LENGTH/2, BOARD_THICKNESS], priority=1)
mesh.AddLine('z', np.linspace(0, BOARD_THICKNESS, 6))

# ═══════════════════════════════════════════════════════════════════════
# GROUND PLANE  (right of keepout strip, full board length)
# ═══════════════════════════════════════════════════════════════════════

gnd = CSX.AddMetal('ground_plane')
gnd.AddBox([GND_X,        -BOARD_LENGTH/2, BOARD_THICKNESS],
           [BOARD_WIDTH/2,  BOARD_LENGTH/2, BOARD_THICKNESS], priority=10)

# ═══════════════════════════════════════════════════════════════════════
# IFA TRACES
#
#  Geometry (looking at board top face, Y axis pointing up):
#
#   ARM_SPINE_X      GND_X
#   │← SC_STUB_LENGTH →│
#   ┌────────────────────┐  SC_STUB_TOP  ─┐ TOP_MARGIN from board top
#   └────────────────────┘  SC_STUB_BOT = ARM_TOP
#   │  (FEED_SEPARATION)
#   ┌──────────────┐        FEED_STUB_TOP
#   └──────────────┘[PORT]  FEED_STUB_BOT
#   │  (spine continues down INIT_LENGTH)
#   ├─────────────────┐     first U top bar
#   │                 │     right column (MEANDER_V_GAP gap on left)
#   ├─────────────────┘     first U bottom bar
#   │  (spine continues TAIL_LENGTH to open end)
#
#  The U-turns are OPEN on the LEFT side — the arm spine connects them
#  externally via the initial spine and inter-meander spine segments.
#  There is NO full-height spine box that would close the U into a rectangle.
# ═══════════════════════════════════════════════════════════════════════

ifa = CSX.AddMetal('ifa')
ex, ey = [GND_X], []   # metal edge X and Y positions for fine meshing

def T(x0, y0, x1, y1):
    """Add a metal trace and record its boundary coordinates."""
    ifa.AddBox([min(x0,x1), min(y0,y1), BOARD_THICKNESS],
               [max(x0,x1), max(y0,y1), BOARD_THICKNESS], priority=10)
    ex.extend([x0, x1]);  ey.extend([y0, y1])

# Short-circuit stub (arm spine left edge → GND plane)
T(ARM_SPINE_X, SC_STUB_BOT, GND_X, SC_STUB_TOP)

# Feed stub (arm spine left edge → port gap left edge)
T(ARM_SPINE_X, FEED_STUB_BOT, PORT_LEFT, FEED_STUB_TOP)

# ── Arm: built as separate segments so the U interior has NO left wall ──

# Initial spine segment (from ARM_TOP downward)
y = ARM_TOP
T(ARM_SPINE_X, y - INIT_LENGTH, ARM_SPINE_RIGHT, y)
y -= INIT_LENGTH

# U-turns
for i in range(N_MEANDERS):
    u_top = y
    u_top_bot = u_top - ARM_TRACE_WIDTH
    u_gap_bot = u_top_bot - MEANDER_V_GAP
    u_bot_top = u_gap_bot
    u_bot_bot = u_gap_bot - ARM_TRACE_WIDTH

    T(ARM_SPINE_X, u_top_bot, MEANDER_RIGHT_X, u_top)            # top bar (full width)
    T(MEANDER_RIGHT_X - ARM_TRACE_WIDTH, u_gap_bot, MEANDER_RIGHT_X, u_top_bot)  # right col
    T(ARM_SPINE_X, u_bot_bot, MEANDER_RIGHT_X, u_bot_top)        # bottom bar (full width)

    y = u_bot_bot
    if i < N_MEANDERS - 1:
        T(ARM_SPINE_X, y - MEANDER_H_GAP, ARM_SPINE_RIGHT, y)   # spine between Us
        y -= MEANDER_H_GAP

# Tail spine (open end of arm)
T(ARM_SPINE_X, y - TAIL_LENGTH, ARM_SPINE_RIGHT, y)
ARM_BOTTOM = y - TAIL_LENGTH

assert ARM_BOTTOM > -BOARD_LENGTH/2 + BOTTOM_MARGIN, \
    (f"Arm bottom {ARM_BOTTOM:.1f} mm exits board. "
     f"Reduce N_MEANDERS, MEANDER_V_GAP, INIT_LENGTH, or TAIL_LENGTH.")

# ═══════════════════════════════════════════════════════════════════════
# LUMPED PORT  (fixed 0.5 mm gap between feed stub right end and GND)
# ═══════════════════════════════════════════════════════════════════════

port = FDTD.AddLumpedPort(port_nr=1, R=50,
    start=[PORT_LEFT, FEED_STUB_BOT, BOARD_THICKNESS],
    stop= [GND_X,     FEED_STUB_TOP, BOARD_THICKNESS],
    p_dir='x', excite=True, priority=5)
ex.extend([PORT_LEFT, GND_X]);  ey.extend([FEED_STUB_BOT, FEED_STUB_TOP])

# ═══════════════════════════════════════════════════════════════════════
# MESH
# ═══════════════════════════════════════════════════════════════════════

mesh.AddLine('x', SmoothMeshLines(sorted(set(ex)), 0.5))
mesh.AddLine('y', SmoothMeshLines(sorted(set(ey)), 0.5))
mesh.SmoothMeshLines('all', MESH_RESOLUTION, 1.4)

nf2ff = FDTD.CreateNF2FFBox()

nx, ny, nz = [len(mesh.GetLines(d)) for d in 'xyz']
print(f"  Mesh X={nx} Y={ny} Z={nz}  cells≈{(nx-1)*(ny-1)*(nz-1):,}  res={MESH_RESOLUTION:.1f}mm")

# ═══════════════════════════════════════════════════════════════════════
# GEOMETRY VERIFICATION
# ═══════════════════════════════════════════════════════════════════════

CSX.Write2XML(Sim_CSX)
print(f"\n  SC stub   y=[{SC_STUB_BOT:.2f}, {SC_STUB_TOP:.2f}]  x=[{ARM_SPINE_X:.2f}, {GND_X:.2f}]")
print(f"  Feed stub y=[{FEED_STUB_BOT:.2f}, {FEED_STUB_TOP:.2f}]  x=[{ARM_SPINE_X:.2f}, {PORT_LEFT:.2f}]")
print(f"  Port gap  x=[{PORT_LEFT:.2f}, {GND_X:.2f}]  ({PORT_WIDTH} mm wide)")
print(f"  Arm spine x=[{ARM_SPINE_X:.2f}, {ARM_SPINE_RIGHT:.2f}]  y=[{ARM_BOTTOM:.2f}, {ARM_TOP:.2f}]")
print(f"  Meander U x right edge={MEANDER_RIGHT_X:.2f}  (GND at {GND_X:.2f}, gap={GND_X-MEANDER_RIGHT_X:.1f}mm)")

try:
    subprocess.Popen(['AppCSXCAD', Sim_CSX]).wait()
except FileNotFoundError:
    pass

input("\nPress [ENTER] to run FDTD, Ctrl+C to abort.\n")

# ═══════════════════════════════════════════════════════════════════════
# RUN
# ═══════════════════════════════════════════════════════════════════════

if not post_proc_only:
    FDTD.Run(Sim_Path, cleanup=True)

# ═══════════════════════════════════════════════════════════════════════
# POST-PROCESSING
# ═══════════════════════════════════════════════════════════════════════

FMIN = 800e6
FMAX = CENTER_FREQUENCY + FC_HALF_BANDWIDTH
f = np.linspace(FMIN, FMAX, 501)
port.CalcPort(Sim_Path, f)

s11    = port.uf_ref / port.uf_inc
s11_dB = 20*np.log10(np.abs(s11) + 1e-30)
Zin    = port.uf_tot / port.if_tot
rez    = np.real(Zin)
imz    = np.imag(Zin)

idx    = np.argmin(s11_dB)
print(f"\n  S11 min : {f[idx]/1e6:.0f} MHz  {s11_dB[idx]:.1f} dB  "
      f"Re={rez[idx]:.0f} Ω  Im={imz[idx]:.0f} Ω")

fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)

ax1.plot(f/1e6, s11_dB, 'royalblue', lw=2, label='S11')
ax1.axvline(915, color='crimson', ls='--', lw=1.5, label='915 MHz')
ax1.axhline(-10, color='gray',  ls=':', lw=1)
ax1.axhline(-15, color='green', ls=':', lw=1, label='−15 dB goal')
ax1.scatter(f[idx]/1e6, s11_dB[idx], color='crimson', zorder=5,
            label=f'{f[idx]/1e6:.0f} MHz, {s11_dB[idx]:.1f} dB')
ax1.set(xlabel='Frequency (MHz)', ylabel='S11 (dB)',
        title=f'S11  N={N_MEANDERS}  V_gap={MEANDER_V_GAP}mm  feed_sep={FEED_SEPARATION}mm',
        xlim=[FMIN/1e6, FMAX/1e6], ylim=[-40, 5])
ax1.grid(True, alpha=0.35);  ax1.legend(fontsize=8)

ax2.plot(f/1e6, rez, 'k-', lw=2, label='Re{Zin}')
ax2.plot(f/1e6, imz, 'r--', lw=2, label='Im{Zin}')
ax2.axvline(915, color='royalblue', ls='--', lw=1.5)
ax2.axhline(50,  color='green',     ls=':', lw=1.2, label='50 Ω')
ax2.axhline(0,   color='gray',      ls='-', lw=0.8)
ax2.axvline(f[idx]/1e6, color='crimson', ls=':', lw=1.5,
            label=f'S11 min @ {f[idx]/1e6:.0f} MHz  Re={rez[idx]:.0f} Ω')
ax2.set(xlabel='Frequency (MHz)', ylabel='Impedance (Ω)',
        xlim=[FMIN/1e6, FMAX/1e6], ylim=[-200, 300])
ax2.grid(True, alpha=0.35);  ax2.legend(fontsize=8)

plt.savefig(os.path.join(Sim_Path, 's11_impedance.png'), dpi=150, bbox_inches='tight')
plt.show()