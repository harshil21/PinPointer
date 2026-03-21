import os
import subprocess
import numpy as np
import matplotlib.pyplot as plt
from openEMS import openEMS
from CSXCAD import ContinuousStructure
from CSXCAD.SmoothMeshLines import SmoothMeshLines
from openEMS.physical_constants import EPS0, C0

# 0.  PATHS & MODE
Sim_Path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'manual_sim')
os.makedirs(Sim_Path, exist_ok=True)
Sim_CSX        = os.path.join(Sim_Path, 'ifa_915.xml')
post_proc_only = False

# 1.  FREQUENCY
CENTER_FREQUENCY = 915e6
f0 = CENTER_FREQUENCY    # Gaussian center [Hz]
fc = 1e9  # 20dB corner frequency [Hz]

# 2.  BOARD & MATERIAL
UNIT = 1e-3  # mm
BOARD_WIDTH = 31.2 
BOARD_LENGTH = 65.3 
BOARD_THICKNESS = 1.6
SUBSTRATE_EPSR = 4.6
SUBSTRATE_LOSS_TAN = 0.015
SUBSTRATE_KAPPA = SUBSTRATE_LOSS_TAN * 2*np.pi*CENTER_FREQUENCY * EPS0 * SUBSTRATE_EPSR
KEEPOUT_ZONE = 10.32  # keepout zone around the feed point

IFA_FEED_WIDTH = 2.0  # feed width
IFA_H = 7  # height of short circuit stub
IFA_RADIATING_ELEMENT_LENGTH = 28.0  # length of the radiating element
IFA_SHORT_CIRCUIT_WIDTH = 2.0  # width of the short circuit stub
IFA_RADIATING_ELEMENT_WIDTH = 1.0  # width of the radiating element
IFA_FEED_POINT_RELATIVE = 4.0  # distance of feed point from short circuit stub

sim_box_size = np.array([200, 200, 200])  # 200mm^3 box

FDTD = openEMS(NrTS=90000, MaxTime=120, CoordSystem=0)  # Limit the simulation to 90k timesteps or 120 seconds, use cartesian coordinates
FDTD.SetGaussExcite(f0, fc)
# MUR - Simple absorbing boundary condition
FDTD.SetBoundaryCond(['MUR', 'MUR', 'MUR', 'MUR', 'MUR', 'MUR'])

# Setup CSXCAD geometry & mesh
CSX = ContinuousStructure()
FDTD.SetCSX(CSX)
mesh = CSX.GetGrid()
mesh.SetDeltaUnit(UNIT)
mesh_res = C0/(f0+fc)/UNIT/20  # maybe remove this? 

# initialize the mesh with the "air-box" dimensions
# We center the box at the origin, so we go from -box_size/2 to +box_size/2 in each dimension
mesh.AddLine('x', [-sim_box_size[0] / 2, sim_box_size[0] / 2])
mesh.AddLine('y', [-sim_box_size[1] / 2, sim_box_size[1] / 2])
mesh.AddLine('z', [-sim_box_size[2] / 2, sim_box_size[2] / 2])

# Create substrate:
substrate = CSX.AddMaterial('substrate', epsilon=SUBSTRATE_EPSR, kappa=SUBSTRATE_KAPPA)
start = [-BOARD_WIDTH / 2, -BOARD_LENGTH / 2, 0]
stop  = [ BOARD_WIDTH / 2, BOARD_LENGTH / 2, BOARD_THICKNESS]
substrate.AddBox(start, stop, priority=1)

# add extra cells to discretize the substrate thickness
mesh.AddLine('z', np.linspace(0, BOARD_THICKNESS, 6))

# Create ground plane
groundplane = CSX.AddMetal('groundplane')  # perfect electric conductor (PEC)
start = [-BOARD_WIDTH / 2,  -BOARD_LENGTH / 2, BOARD_THICKNESS]
stop  = [ BOARD_WIDTH / 2,   BOARD_LENGTH / 2 - KEEPOUT_ZONE, BOARD_THICKNESS]
groundplane.AddBox(start, stop, priority=10)

# Create ifa
ifa = CSX.AddMetal('ifa')  # perfect electric conductor (PEC)
tl = np.array([-BOARD_WIDTH / 2, BOARD_LENGTH / 2 - KEEPOUT_ZONE, BOARD_THICKNESS])  # translate

# feed element
start = np.array([IFA_FEED_POINT_RELATIVE + 1, 0.5, 0]) + tl
stop = start + np.array([IFA_FEED_WIDTH, IFA_H, 0])
ifa.AddBox(start.tolist(), stop.tolist(), priority=10)

# short circuit stub
start = np.array([IFA_FEED_POINT_RELATIVE, 0, 0]) + tl
stop  = start + np.array([-IFA_SHORT_CIRCUIT_WIDTH, IFA_H + 0.5, 0])
ifa.AddBox(start.tolist(), stop.tolist(), priority=10)

# radiating element
start = np.array([(IFA_FEED_POINT_RELATIVE - IFA_SHORT_CIRCUIT_WIDTH), IFA_H + 0.5, 0]) + tl
stop  = start + np.array([IFA_RADIATING_ELEMENT_LENGTH, IFA_RADIATING_ELEMENT_WIDTH, 0])
ifa.AddBox(start.tolist(), stop.tolist(), priority=10)

# Add a mesh:

ifa_edges_x = [
    IFA_FEED_POINT_RELATIVE - IFA_SHORT_CIRCUIT_WIDTH + tl[0],  # Start of short circuit stub
    IFA_FEED_POINT_RELATIVE + tl[0],  # End of short circuit stub
    IFA_FEED_POINT_RELATIVE + 1 + tl[0],  # Start of feed element
    IFA_FEED_POINT_RELATIVE + 1 + tl[0] + IFA_FEED_WIDTH,  # End of feed element
    IFA_RADIATING_ELEMENT_LENGTH + IFA_FEED_POINT_RELATIVE - IFA_SHORT_CIRCUIT_WIDTH + tl[0],  # End of radiating element
]
ifa_edges_y = [
    tl[1],  # Base of short circuit stub
    tl[1] + 0.5,  # Feed element bottom
    tl[1] + IFA_H + 0.5,  # Top of short circuit stub and feed element
    tl[1] + IFA_H + IFA_RADIATING_ELEMENT_WIDTH + 0.5,  # Top of radiating element
]
mesh.AddLine('x', SmoothMeshLines(ifa_edges_x, 0.5))
mesh.AddLine('y', SmoothMeshLines(ifa_edges_y, 0.5))

# Apply excitation & lumped port (current source)
start = np.array([IFA_FEED_POINT_RELATIVE + 1, 0, 0]) + tl
stop  = start + np.array([IFA_FEED_WIDTH, 0.5, 0])
port = FDTD.AddLumpedPort(port_nr=1, R=50, start=start, stop=stop,
                  p_dir='y', excite=True, priority=5)

# Finalize mesh:
mesh.SmoothMeshLines('all', mesh_res, 1.4)

# Setup NF2FF box (for far-field calculations)
nf2ff = FDTD.CreateNF2FFBox()

x_lines = len(mesh.GetLines('x'))
y_lines = len(mesh.GetLines('y'))
z_lines = len(mesh.GetLines('z'))
total   = (x_lines-1) * (y_lines-1) * (z_lines-1)
print(f"  Mesh lines      : X={x_lines}  Y={y_lines}  Z={z_lines}")
print(f"  Total FDTD cells: ~{total:,}  (target: 40k–120k)")
print(f"  mesh_res        : {mesh_res:.1f} mm  (λ/20 @ {(f0+fc)/1e6:.0f} MHz)")



# Write the geometry to XML and open in AppCSXCAD for verification
CSX.Write2XML(os.path.join(Sim_Path, Sim_CSX))

try:
    subprocess.Popen(['AppCSXCAD', Sim_CSX]).wait()
except FileNotFoundError:
    print("  AppCSXCAD not found — skipping.")

input("\nGeometry correct? Press [ENTER] to start FDTD. Or exit with Ctrl+C\n")

if not post_proc_only:
    print(f"Running FDTD in: {Sim_Path}")
    FDTD.Run(Sim_Path, cleanup=True)

# Post Processing:
print("\nCalculating port quantities...")
PLOT_START_FREQ = 300e6
PLOT_STOP_FREQ = 3000e6
f = np.linspace(PLOT_START_FREQ, PLOT_STOP_FREQ, 651)
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

fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)

ax1.plot(f/1e6, s11_dB, 'royalblue', lw=2, label='S11')
ax1.axvline(915, color='crimson', ls='--', lw=1.5, label='915 MHz target')
ax1.axhline(-10, color='gray',   ls=':',  lw=1.0, label='−10 dB')
ax1.axhline(-15, color='green',  ls=':',  lw=1.0, label='−15 dB goal')
ax1.scatter(f_s11/1e6, s11_min, color='crimson', zorder=5,
            label=f'S11 min: {f_s11/1e6:.0f} MHz, {s11_min:.1f} dB')
# if f_series:
#     ax1.axvline(f_series/1e6, color='orange', ls=':', lw=1.5,
#                 label=f'Im↓=0 series: {f_series/1e6:.0f} MHz')
ax1.set_xlabel('Frequency (MHz)')
ax1.set_ylabel('S11 (dB)')
ax1.set_title(f'S11')
ax1.set_xlim([PLOT_START_FREQ/1e6, PLOT_STOP_FREQ/1e6])
ax1.set_ylim([-40, 5])
ax1.grid(True, alpha=0.35)
ax1.legend(fontsize=8)

ax2.plot(f/1e6, rez, 'k-',  lw=2, label='Re{Zin}')
ax2.plot(f/1e6, imz, 'r--', lw=2, label='Im{Zin}')
ax2.axvline(915,   color='royalblue', ls='--', lw=1.5, label='915 MHz')
ax2.axhline(50,    color='green',     ls=':',  lw=1.2, label='50 Ω target')
ax2.axhline(0,     color='gray',      ls='-',  lw=0.8)
# if f_series:
#     ax2.axvline(f_series/1e6, color='orange', ls=':', lw=1.8,
#                 label=f'Im↓=0 @ {f_series/1e6:.0f} MHz  Re={R_series:.0f} Ω')
ax2.set_xlabel('Frequency (MHz)')
ax2.set_ylabel('Impedance (Ω)')
ax2.set_title('Zin — Im DESCENDING through 0 = IFA series resonance  (want: Re=50 Ω at 915 MHz)')
ax2.set_xlim([PLOT_START_FREQ/1e6, PLOT_STOP_FREQ/1e6])
ax2.set_ylim([-200, 300])
ax2.grid(True, alpha=0.35)
ax2.legend(fontsize=8)

plt.savefig(os.path.join(Sim_Path, 's11_impedance.png'), dpi=150, bbox_inches='tight')
plt.show()