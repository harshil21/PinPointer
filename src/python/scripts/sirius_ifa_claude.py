# -*- coding: utf-8 -*-
"""
915 MHz Straight IFA — 4-Layer Raspberry Pi Hat PCB  (v10)
openEMS FDTD  (Python API)

═══════════════════════════════════════════════════════════════════
  COMPLETE DIAGNOSIS — WHY PREVIOUS VERSIONS FAILED
═══════════════════════════════════════════════════════════════════

  v9 BUG 1 — ASCENDING Im=0 detection is WRONG for an IFA
    For a simple dipole: Im < 0 below resonance → ascending Im=0
    = series resonance.  But an IFA has a SHORT stub, which makes
    the input INDUCTIVE (Im > 0) at low frequencies.  So Im starts
    positive, then DESCENDS through 0 at the series resonance.
      Descending Im=0  →  SERIES resonance  (Re = low-medium Ω)
      Ascending  Im=0  →  ANTI-resonance    (Re = very high Ω)
    PROOF from v4: descending Im=0 → Re=232 Ω, S11=-3.7 dB.
    Check: (232-50)/(232+50)=0.645 → -3.8 dB ✓  Perfect match.
    v9's ascending detection → Re=4408 Ω (anti-resonance — nonsensical).

  v9 BUG 2 — MUR boundaries too close to antenna
    SimBox = 2×board = 62 mm in X.  λ/4 at 315 MHz = 238 mm.
    Boundary was only 31 mm from the antenna — severe reflection
    that shifted the apparent resonant frequency by ~200 MHz.
    Fix: back to PML_8 with 65 mm padding (confirmed working in v4).

  v9 BUG 3 — Inner GND metal box not on any Z mesh line
    linspace(0, 1.6, 5) = [0, 0.4, 0.8, 1.2, 1.6].
    z_gnd_int = 1.3546 is between 1.2 and 1.6 — no mesh line there.
    openEMS cannot map the 2D metal → "Unused primitive" warning,
    the box is silently dropped, changing the simulation.
    Fix: keep z_gnd_int as a MESH LINE (needed for Z cell count)
    but do NOT create any metal object at that z-coordinate.

  v5/v6 REMEMBERED — z_gnd_int mesh line must stay
    With only [0, 1.6] as Z hard lines, SmoothMeshLines generates
    only ~10 Z cells total. PML_8 consumes 8 per side → NF2FF crash
    or metal-cell mapping failure.  The 0.245 mm cell from z_gnd_int
    to z_top forces SmoothMeshLines to chain-grow ~12 transition cells
    above z_top → ~26 total Z cells → PML_8 + NF2FF fit comfortably.

═══════════════════════════════════════════════════════════════════
  TUNING GUIDE
═══════════════════════════════════════════════════════════════════

  arm_total  → resonant frequency
    Series resonance (descending Im=0) below 915 MHz → DECREASE arm_total
    Series resonance above 915 MHz                  → INCREASE arm_total
    Rule of thumb: ±1 mm ≈ ∓9 MHz

  ifa_fp  → input resistance Re(Zin) at series resonance
    Re > 50 Ω  →  INCREASE ifa_fp  (feed further from short stub)
    Re < 50 Ω  →  DECREASE ifa_fp  (feed closer to short stub)
    Exact formula printed after each run. tail_length auto-adjusts.

  Goal: S11 < −15 dB at 915 MHz
    Requires: descending Im=0 at 915 MHz  AND  Re(Zin) = 50 Ω.

═══════════════════════════════════════════════════════════════════
  PCB LAYOUT NOTE
═══════════════════════════════════════════════════════════════════
  Apply the 15 mm keepout to ALL four copper layers in KiCad:
    F.Cu,  GND inner,  PWR inner,  B.Cu
  50 Ω microstrip on F.Cu over PrePreg (h=0.2104 mm, εr=4.6):
    trace width ≈ 0.39 mm
"""

import os
import subprocess
import numpy as np
import matplotlib.pyplot as plt
from openEMS import openEMS
from CSXCAD import ContinuousStructure
from openEMS.physical_constants import *

# ═══════════════════════════════════════════════════════════════════════
# 0.  PATHS & MODE
# ═══════════════════════════════════════════════════════════════════════
Sim_Path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'sim_ifa_915')
os.makedirs(Sim_Path, exist_ok=True)
Sim_CSX        = os.path.join(Sim_Path, 'ifa_915.xml')
post_proc_only = False

# ═══════════════════════════════════════════════════════════════════════
# 1.  FREQUENCY  — wide Gaussian for fast convergence
# ═══════════════════════════════════════════════════════════════════════
f_target = 915e6
f0       = 915e6    # Gaussian center [Hz]
fc       = 600e6    # half-bandwidth → spans 315–1515 MHz
                    # Wide bandwidth = short pulse = fast ring-down to -40 dB

# ═══════════════════════════════════════════════════════════════════════
# 2.  BOARD & MATERIAL
# ═══════════════════════════════════════════════════════════════════════
unit              = 1e-3
board_w           = 31.244          # X [mm]
board_l           = 65.386          # Y [mm]
substrate_epsR    = 4.6
substrate_lossTan = 0.015           # from KiCad stackup (FR4 loss tangent)
substrate_kappa   = substrate_lossTan * 2*np.pi*f_target * EPS0 * substrate_epsR

z_bot     = 0.0
z_gnd_int = 1.6 - 0.035 - 0.2104   # ≈ 1.3546 mm — Z MESH LINE only, NO metal box
z_top     = 1.6                     # F.Cu / antenna layer

# ═══════════════════════════════════════════════════════════════════════
# 3.  ANTENNA PARAMETERS  ← PRIMARY TUNING KNOBS
# ═══════════════════════════════════════════════════════════════════════
keepout_h  = 15.0
gnd_stop_x = -board_w/2 + keepout_h    # = −0.622 mm

ifa_h      =  8.0   # stub reach in −X into keepout [mm]  (keep fixed)
ifa_fp     = 5.0   # feed-stub Y offset from short  [mm]  ← IMPEDANCE TUNE
                    # Prediction from v4 (arm=45, ifa_fp=9, Re=232 Ω):
                    #   R_total = 232 × sin²(18°) = 22 Ω
                    #   For arm=53: fp_opt ≈ 16 mm to achieve Re=50 Ω
trace_w    =  1.0   # trace width [mm]
port_gap   =  1.0   # lumped-port gap in X [mm]
arm_total  = 59.6   # total arm length short→open [mm]  ← FREQUENCY TUNE
                    # v7 empirical: arm≈53 mm → series res ≈909 MHz

# Derived
x_el        = gnd_stop_x - ifa_h
start_y     = -board_l/2 + 3.0
tail_length = arm_total - ifa_fp - trace_w
arm_end_y   = start_y + arm_total

assert tail_length > 2.0, (
    f"tail_length={tail_length:.1f} mm — reduce ifa_fp ({ifa_fp}) or increase arm_total ({arm_total})")
assert arm_end_y < board_l/2 - 1.5, (
    f"Arm exits board! end={arm_end_y:.1f}, limit={board_l/2-1.5:.1f}")

lam4 = C0 / (4 * f_target) / unit
print("=" * 64)
print(f"  arm_total       : {arm_total:.1f} mm  ← frequency knob")
print(f"  ifa_fp          : {ifa_fp:.1f} mm  ← impedance knob  ({ifa_fp/arm_total*100:.0f}% of arm)")
print(f"  tail_length     : {tail_length:.1f} mm  (auto-derived)")
print(f"  λ/4 at 915 MHz  : {lam4:.1f} mm  (free space)")
print(f"  Velocity factor : {arm_total/lam4:.2f}  (typical FR4 IFA: 0.60–0.75)")
print(f"  Arm Y span      : [{start_y:.1f}, {arm_end_y:.1f}] mm  (board ±{board_l/2:.1f})")
print("=" * 64)

# ═══════════════════════════════════════════════════════════════════════
# 4.  FDTD ENGINE  — PML_8 (not MUR — MUR was too close at 2×board)
# ═══════════════════════════════════════════════════════════════════════
FDTD = openEMS(NrTS=1000000, EndCriteria=1e-4)
FDTD.SetGaussExcite(f0, fc)
FDTD.SetBoundaryCond(['PML_8'] * 6)

CSX  = ContinuousStructure()
FDTD.SetCSX(CSX)
mesh = CSX.GetGrid()
mesh.SetDeltaUnit(unit)

# ═══════════════════════════════════════════════════════════════════════
# 5.  SIMULATION BOX + Z MESH
# ═══════════════════════════════════════════════════════════════════════
padding  = 65.0     # mm — proven working in v4/v7

mesh.AddLine('x', [-board_w/2 - padding,  board_w/2 + padding])
mesh.AddLine('y', [-board_l/2 - padding,  board_l/2 + padding])
mesh.AddLine('z', [-padding,               z_top + padding])

# ── Z hard lines: z_gnd_int is a MESH LINE, NOT a metal object ────────
#
#  The 0.245 mm gap between z_gnd_int (1.3546) and z_top (1.600) forces
#  SmoothMeshLines to chain-grow transition cells above z_top, giving
#  enough Z cells for PML_8 + NF2FF.  See v5/v6 disaster for what
#  happens if this line is removed.
#
#  There is NO metal box at z_gnd_int — that caused "Unused primitive"
#  in v9 because z_gnd_int was between linspace Z lines.
#
mesh.AddLine('z', [z_bot, z_gnd_int, z_top])

# ═══════════════════════════════════════════════════════════════════════
# 6.  SUBSTRATE
# ═══════════════════════════════════════════════════════════════════════
substrate = CSX.AddMaterial('substrate', epsilon=substrate_epsR, kappa=substrate_kappa)
substrate.AddBox(priority=0,
                 start=[-board_w/2, -board_l/2, z_bot],
                 stop= [ board_w/2,  board_l/2,  z_top])

# ═══════════════════════════════════════════════════════════════════════
# 7.  COPPER LAYERS  — F.Cu and B.Cu with keepout (inner GND omitted)
# ═══════════════════════════════════════════════════════════════════════
mesh_res = C0 / (f0 + fc) / unit / 20   # λ/20 @ highest freq ≈ 13.2 mm

gnd_top = CSX.AddMetal('gnd_top')
gnd_top.AddBox(priority=10,
               start=[gnd_stop_x, -board_l/2, z_top],
               stop= [board_w/2,   board_l/2,  z_top])
FDTD.AddEdges2Grid(dirs='xy', properties=gnd_top, metal_edge_res=mesh_res/2)

gnd_bot = CSX.AddMetal('gnd_bot')
gnd_bot.AddBox(priority=10,
               start=[gnd_stop_x, -board_l/2, z_bot],
               stop= [board_w/2,   board_l/2,  z_bot])
FDTD.AddEdges2Grid(dirs='xy', properties=gnd_bot, metal_edge_res=mesh_res/2)

# ═══════════════════════════════════════════════════════════════════════
# 8.  IFA ANTENNA TRACES  (F.Cu, z = z_top)
# ═══════════════════════════════════════════════════════════════════════
ifa = CSX.AddMetal('ifa')

# Short-circuit stub — connects arm to GND pour at x=gnd_stop_x
ifa.AddBox(priority=10,
           start=[x_el,       start_y,           z_top],
           stop= [gnd_stop_x, start_y + trace_w, z_top])

# Feed stub — gap at right end (lumped port goes here)
ifa.AddBox(priority=10,
           start=[x_el,                  start_y + ifa_fp,           z_top],
           stop= [gnd_stop_x - port_gap, start_y + ifa_fp + trace_w, z_top])

# Vertical radiating arm
ifa.AddBox(priority=10,
           start=[x_el,           start_y,             z_top],
           stop= [x_el + trace_w, start_y + arm_total, z_top])

FDTD.AddEdges2Grid(dirs='xy', properties=ifa, metal_edge_res=mesh_res/2)

# Explicit hard line at gnd_stop_x — guarantees short stub shares a
# mesh cell boundary with the GND pour (electrical connection)
mesh.AddLine('x', [gnd_stop_x])

# ═══════════════════════════════════════════════════════════════════════
# 9.  LUMPED PORT  (50 Ω, in X direction at feed stub gap)
# ═══════════════════════════════════════════════════════════════════════
p_start = [gnd_stop_x - port_gap, start_y + ifa_fp,            z_top]
p_stop  = [gnd_stop_x,             start_y + ifa_fp + trace_w, z_top]
port = FDTD.AddLumpedPort(1, 50, p_start, p_stop, 'x', 1.0,
                          priority=50, edges2grid='xy')

# ═══════════════════════════════════════════════════════════════════════
# 10.  SMOOTH MESH + NF2FF
# ═══════════════════════════════════════════════════════════════════════
mesh.SmoothMeshLines('all', mesh_res, 1.4)
nf2ff = FDTD.CreateNF2FFBox()

x_lines = len(mesh.GetLines('x'))
y_lines = len(mesh.GetLines('y'))
z_lines = len(mesh.GetLines('z'))
total   = (x_lines-1) * (y_lines-1) * (z_lines-1)
print(f"  Mesh lines      : X={x_lines}  Y={y_lines}  Z={z_lines}")
print(f"  Total FDTD cells: ~{total:,}  (target: 40k–120k)")
print(f"  mesh_res        : {mesh_res:.1f} mm  (λ/20 @ {(f0+fc)/1e6:.0f} MHz)")

# ═══════════════════════════════════════════════════════════════════════
# 11.  GEOMETRY CHECK — AppCSXCAD
# ═══════════════════════════════════════════════════════════════════════
CSX.Write2XML(Sim_CSX)

print("\n" + "═"*64)
print("  Geometry Checklist")
print(f"   ✓ Arm VERTICAL  x=[{x_el:.3f}, {x_el+trace_w:.3f}] mm")
print(f"   ✓ Arm Y span    [{start_y:.2f}, {arm_end_y:.2f}] mm  (board ±{board_l/2:.1f})")
print(f"   ✓ Short stub    y={start_y:.2f}  x=[{x_el:.3f},{gnd_stop_x:.3f}]  MUST touch GND")
print(f"   ✓ Feed  stub    y={start_y+ifa_fp:.2f}  x=[{x_el:.3f},{gnd_stop_x-port_gap:.3f}]")
print(f"   ✓ Port  gap     x=[{gnd_stop_x-port_gap:.3f},{gnd_stop_x:.3f}]")
print(f"   ✓ GND pour      x=[{gnd_stop_x:.3f},{board_w/2:.3f}]  (F.Cu + B.Cu)")
print(f"   ✓ z_gnd_int={z_gnd_int:.4f} mm  MESH LINE only — NO metal box")
print("═"*64)
print("\n  Key check: zoom into bottom of arm in AppCSXCAD.")
print("  Short stub must visibly SHARE the boundary at x={:.3f} mm".format(gnd_stop_x))
print("  with the GND pour — no visible gap between them.")

try:
    subprocess.Popen(['AppCSXCAD', Sim_CSX]).wait()
except FileNotFoundError:
    print("  AppCSXCAD not found — skipping.")

input("\nGeometry correct? Press [ENTER] to start FDTD...\n")

# ═══════════════════════════════════════════════════════════════════════
# 12.  RUN FDTD
# ═══════════════════════════════════════════════════════════════════════
if not post_proc_only:
    print(f"Running FDTD in: {Sim_Path}")
    FDTD.Run(Sim_Path, cleanup=True)

# ═══════════════════════════════════════════════════════════════════════
# 13.  POST-PROCESSING
# ═══════════════════════════════════════════════════════════════════════
print("\nCalculating port quantities...")
f      = np.linspace(300e6, 1600e6, 651)
port.CalcPort(Sim_Path, f)

s11    = port.uf_ref / port.uf_inc
s11_dB = 20.0 * np.log10(np.abs(s11) + 1e-30)
Zin    = port.uf_tot / port.if_tot
P_in   = 0.5 * np.real(port.uf_tot * np.conj(port.if_tot))

imz = np.imag(Zin)
rez = np.real(Zin)

# ── PRIMARY: S11 minimum ─────────────────────────────────────────────
idx_s11 = np.argmin(s11_dB)
f_s11   = f[idx_s11]
s11_min = s11_dB[idx_s11]
R_s11   = rez[idx_s11]
X_s11   = imz[idx_s11]

# ── SECONDARY: Descending Im=0 = IFA series resonance ────────────────
#
#  For an IFA, the shorted stub makes the input inductive at low f
#  (Im > 0).  The series resonance is where Im DESCENDS through zero.
#  (This is the opposite of a simple dipole/monopole.)
#
#  Verification from v4: descending Im=0 at 918 MHz → Re=232 Ω,
#  S11=-3.7 dB.  (232-50)/(232+50) = 0.645 → -3.8 dB ✓
#
descending = np.where((imz[:-1] > 0) & (imz[1:] <= 0))[0]

if len(descending) > 0:
    fd_zeros, rd_zeros = [], []
    for k in descending:
        alpha = -imz[k] / (imz[k+1] - imz[k] + 1e-30)
        fd_zeros.append(f[k] + alpha*(f[k+1]-f[k]))
        rd_zeros.append(rez[k] + alpha*(rez[k+1]-rez[k]))
    near_d     = np.argmin(np.abs(np.array(fd_zeros) - f_target))
    f_series   = fd_zeros[near_d]
    R_series   = rd_zeros[near_d]
    idx_series = descending[near_d]
    s11_series = s11_dB[idx_series]
else:
    f_series   = None
    R_series   = None
    idx_series = idx_s11
    s11_series = s11_min

# Use series resonance for tuning guidance
f_res   = f_series if f_series else f_s11
R_res   = R_series if R_series else R_s11

print(f"\n{'═'*64}")
print(f"  S11 minimum         : {f_s11/1e6:7.1f} MHz  S11={s11_min:.1f} dB  "
      f"Re={R_s11:.0f} Ω  Im={X_s11:.0f} Ω")
if f_series:
    print(f"  Series res (Im↓=0)  : {f_series/1e6:7.1f} MHz  Re={R_series:.0f} Ω  "
          f"S11={s11_series:.1f} dB")
else:
    print(f"  Series res (Im↓=0)  : not found in 300–1600 MHz sweep")
print(f"{'═'*64}")

df_MHz = (f_res - f_target) / 1e6
dR     = R_res - 50.0

print("\nTUNING GUIDANCE:")
if abs(df_MHz) > 8:
    dmm  = abs(df_MHz) / 9.0
    verb = "DECREASE" if df_MHz < 0 else "INCREASE"
    print(f"  STEP 1  Frequency  {df_MHz:+.0f} MHz  →  {verb} arm_total by ≈{dmm:.1f} mm")
    print(f"          (current arm_total = {arm_total:.1f} mm)")
else:
    print(f"  STEP 1  Frequency ✓  Δ = {df_MHz:+.1f} MHz")

if abs(dR) > 5:
    ang     = np.pi / 2.0 * ifa_fp / arm_total
    sin_opt = np.clip(np.sin(ang) * np.sqrt(max(R_res/50.0, 1e-3)), 0.0, 0.999)
    fp_opt  = arm_total * (2.0/np.pi) * np.arcsin(sin_opt)
    new_t   = arm_total - fp_opt - trace_w
    verb    = "INCREASE" if dR > 0 else "DECREASE"
    print(f"  STEP 2  Impedance  Re(Zin)={R_res:.0f} Ω  →  {verb} ifa_fp")
    print(f"          Current: {ifa_fp:.1f} mm  →  Suggested: {fp_opt:.1f} mm")
    if new_t > 2.0:
        print(f"          (tail_length: {tail_length:.1f} → {new_t:.1f} mm)")
    else:
        print(f"  ⚠  Suggested fp={fp_opt:.1f} gives tail={new_t:.1f} mm — too short; "
              f"increase arm_total first.")
else:
    print(f"  STEP 2  Impedance ✓  Re(Zin) = {R_res:.0f} Ω")

if abs(df_MHz) <= 8 and s11_min < -10 and abs(dR) <= 5:
    print(f"\n  🎯  GOAL REACHED — S11 = {s11_min:.1f} dB @ {f_s11/1e6:.0f} MHz")

# ── Plots ─────────────────────────────────────────────────────────────
fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)

ax1.plot(f/1e6, s11_dB, 'royalblue', lw=2, label='S11')
ax1.axvline(915, color='crimson', ls='--', lw=1.5, label='915 MHz target')
ax1.axhline(-10, color='gray',   ls=':',  lw=1.0, label='−10 dB')
ax1.axhline(-15, color='green',  ls=':',  lw=1.0, label='−15 dB goal')
ax1.scatter(f_s11/1e6, s11_min, color='crimson', zorder=5,
            label=f'S11 min: {f_s11/1e6:.0f} MHz, {s11_min:.1f} dB')
if f_series:
    ax1.axvline(f_series/1e6, color='orange', ls=':', lw=1.5,
                label=f'Im↓=0 series: {f_series/1e6:.0f} MHz')
ax1.set_xlabel('Frequency (MHz)')
ax1.set_ylabel('S11 (dB)')
ax1.set_title(f'S11  [v10]   arm={arm_total:.0f} mm   ifa_fp={ifa_fp:.1f} mm')
ax1.set_xlim([300, 1600])
ax1.set_ylim([-40, 5])
ax1.grid(True, alpha=0.35)
ax1.legend(fontsize=8)

ax2.plot(f/1e6, rez, 'k-',  lw=2, label='Re{Zin}')
ax2.plot(f/1e6, imz, 'r--', lw=2, label='Im{Zin}')
ax2.axvline(915,   color='royalblue', ls='--', lw=1.5, label='915 MHz')
ax2.axhline(50,    color='green',     ls=':',  lw=1.2, label='50 Ω target')
ax2.axhline(0,     color='gray',      ls='-',  lw=0.8)
if f_series:
    ax2.axvline(f_series/1e6, color='orange', ls=':', lw=1.8,
                label=f'Im↓=0 @ {f_series/1e6:.0f} MHz  Re={R_series:.0f} Ω')
ax2.set_xlabel('Frequency (MHz)')
ax2.set_ylabel('Impedance (Ω)')
ax2.set_title('Zin — Im DESCENDING through 0 = IFA series resonance  (want: Re=50 Ω at 915 MHz)')
ax2.set_xlim([300, 1600])
ax2.set_ylim([-200, 300])
ax2.grid(True, alpha=0.35)
ax2.legend(fontsize=8)

plt.savefig(os.path.join(Sim_Path, 's11_impedance.png'), dpi=150, bbox_inches='tight')
plt.show()

# ═══════════════════════════════════════════════════════════════════════
# 14.  FAR-FIELD  (only when S11 < −10 dB)
# ═══════════════════════════════════════════════════════════════════════
if s11_min < -10.0:
    print(f"\nCalculating far-field at {f_s11/1e6:.0f} MHz...")
    theta = np.arange(-180.0, 180.0, 2.0)
    phi   = [0., 90.]
    nf2ff_res = nf2ff.CalcNF2FF(
        Sim_Path, f_s11, theta, phi,
        center=[0.0, 0.0, z_top * unit],
        read_cached=True, outfile='nf2ff_915.h5')
    Prad = nf2ff_res.Prad[0]
    Dmax = nf2ff_res.Dmax[0]
    eta  = 100.0 * Prad / P_in[idx_s11]
    print(f"  Radiated power  : {Prad:.3e} W")
    print(f"  Max directivity : {10*np.log10(Dmax):.1f} dBi")
    print(f"  Radiation eff.  : {eta:.1f} %")
    if eta < 40:
        print("  ⚠  Low efficiency — verify ALL copper layer keepouts in KiCad!")

    E_norm = 20*np.log10(nf2ff_res.E_norm[0]/np.max(nf2ff_res.E_norm[0])) \
             + 10*np.log10(Dmax)
    fig2, ax = plt.subplots(figsize=(8, 6))
    ax.plot(theta, np.squeeze(E_norm[:,0]), 'k-',  lw=2, label='xz-plane (φ=0°)')
    ax.plot(theta, np.squeeze(E_norm[:,1]), 'r--', lw=2, label='yz-plane (φ=90°)')
    ax.set_xlabel('Theta (deg)')
    ax.set_ylabel('Directivity (dBi)')
    ax.set_title(f'Far-Field  {f_s11/1e6:.0f} MHz  '
                 f'Dmax={10*np.log10(Dmax):.1f} dBi  η={eta:.0f}%')
    ax.grid(True, alpha=0.35)
    ax.legend()
    plt.savefig(os.path.join(Sim_Path, 'farfield.png'), dpi=150, bbox_inches='tight')
    plt.show()
else:
    print(f"\nS11={s11_min:.1f} dB — follow tuning guidance above first.")

print(f"\nAll outputs saved to: {Sim_Path}")