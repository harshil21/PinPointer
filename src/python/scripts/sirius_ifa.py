import os
import math
import numpy as np
import matplotlib.pyplot as plt
from openEMS import openEMS
from CSXCAD import ContinuousStructure
from openEMS.physical_constants import *

def export_vtk_fallback(filename, nf2ff_res, theta, phi, scale=1e-3):
    try: os.remove(filename)
    except OSError: pass
    max_E = np.max(nf2ff_res.E_norm[0])
    if max_E == 0: return
    E_far_normalized = nf2ff_res.E_norm[0] / max_E * nf2ff_res.Dmax[0]
    with open(filename, 'w') as fid:
        fid.write("# vtk DataFile Version 3.0\nStructured Grid by python-interface of openEMS\nASCII\nDATASET STRUCTURED_GRID\n")
        fid.write(f"DIMENSIONS 1 {len(theta)} {len(phi)}\nPOINTS {len(theta)*len(phi)} double\n")
        for p_idx, p in enumerate(phi):
            for t_idx, t in enumerate(theta):
                val = E_far_normalized[t_idx, p_idx] * scale
                fid.write(f"{val * math.sin(t) * math.cos(p):e} {val * math.sin(t) * math.sin(p):e} {val * math.cos(t):e}\n")
        fid.write(f"\n\nPOINT_DATA {len(theta)*len(phi)}\n")

# ==========================================
# 1. SETUP & PARAMETERS
# ==========================================
Sim_Path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'simulation_folder')
os.makedirs(Sim_Path, exist_ok=True)

f0 = 915e6
fc = 500e6 

FDTD = openEMS(EndCriteria=1e-4)
FDTD.SetGaussExcite(f0, fc)
FDTD.SetBoundaryCond(['MUR', 'MUR', 'MUR', 'MUR', 'MUR', 'MUR']) 

CSX = ContinuousStructure()
FDTD.SetCSX(CSX)
mesh = CSX.GetGrid()
mesh.SetDeltaUnit(1e-3)
mesh_res = C0/(f0+fc)/1e-3/20

board_width = 31.244
# board_width = 55.244
board_length = 65.386
keepout_h = 10.32
substrate_thickness = 1.6
substrate_epsR = 4.6

# --- TI-STYLE TUNING DIALS ---
ifa_fp = 5.0          # 50-Ohm Match (Distance from short to feed)
ifa_h = 10.0           # Total height of the antenna above GND
trace_w = 1.0         # Width of all traces

# Square-Wave Meander Params
N_teeth = 3           # Number of down-up meander teeth
tooth_drop = 7.0      # How deep the teeth drop towards the ground plane
tooth_gap = 4.0       # Horizontal gap between vertical traces
tail_length = 16.0     # Final horizontal arm at the top
# -----------------------------

z_gnd = substrate_thickness - 0.2104
z_top = substrate_thickness
gnd_stop_y = (board_length/2) - keepout_h 
start_x = -board_width/2 + 1.0 

# ==========================================
# 2. GENERATE PROPERTIES AND PRIMITIVES
# ==========================================
SimBox = np.array([board_width*2, board_length*2, 150])

mesh.AddLine('x', [-SimBox[0]/2, SimBox[0]/2])
mesh.AddLine('y', [-SimBox[1]/2, SimBox[1]/2])
mesh.AddLine('z', [-SimBox[2]/2, SimBox[2]/2])

substrate = CSX.AddMaterial('substrate', epsilon=substrate_epsR)
substrate.AddBox(priority=0, start=[-board_width/2, -board_length/2, 0], stop=[board_width/2, board_length/2, substrate_thickness])
mesh.AddLine('z', np.linspace(0, substrate_thickness, 5).tolist())

gnd = CSX.AddMetal('gnd')
gnd.AddBox(priority=10, start=[-board_width/2, -board_length/2, z_gnd], stop=[board_width/2, gnd_stop_y, z_gnd])

ifa = CSX.AddMetal('ifa')

# --- GENERATE THE ANTENNA SHAPE ---
# 1. Base Pins
ifa.AddBox([start_x, gnd_stop_y, z_gnd], [start_x + trace_w, gnd_stop_y, z_top], priority=10) # Shorting Via
ifa.AddBox([start_x, gnd_stop_y, z_top], [start_x + trace_w, gnd_stop_y + ifa_h, z_top], priority=10) # Shorting Trace
ifa.AddBox([start_x + ifa_fp, gnd_stop_y, z_top], [start_x + ifa_fp + trace_w, gnd_stop_y + ifa_h, z_top], priority=10) # Feed Pin

# 2. Connecting Top trace from Short -> Feed
curr_x = start_x + ifa_fp + trace_w
curr_y = gnd_stop_y + ifa_h
ifa.AddBox([start_x, curr_y - trace_w, z_top], [curr_x, curr_y, z_top], priority=10)

# 3. Generate the TI Square-Wave Teeth
for i in range(N_teeth):
    # Top gap
    ifa.AddBox([curr_x, curr_y - trace_w, z_top], [curr_x + tooth_gap, curr_y, z_top], priority=10)
    curr_x += tooth_gap
    
    # Drop down
    ifa.AddBox([curr_x, curr_y - tooth_drop, z_top], [curr_x + trace_w, curr_y, z_top], priority=10)
    curr_x += trace_w
    
    # Bottom gap
    ifa.AddBox([curr_x, curr_y - tooth_drop, z_top], [curr_x + tooth_gap, curr_y - tooth_drop + trace_w, z_top], priority=10)
    curr_x += tooth_gap
    
    # Rise up
    ifa.AddBox([curr_x, curr_y - tooth_drop, z_top], [curr_x + trace_w, curr_y, z_top], priority=10)
    curr_x += trace_w

# 4. Final Tail
ifa.AddBox([curr_x, curr_y - trace_w, z_top], [curr_x + tail_length, curr_y, z_top], priority=10)

# ==========================================
# 3. MESH AUTOMATION
# ==========================================
FDTD.AddEdges2Grid(dirs='xy', properties=ifa, metal_edge_res=0.5)
FDTD.AddEdges2Grid(dirs='xy', properties=gnd, metal_edge_res=0.5)

mesh.AddLine('z', [z_gnd, z_top])
mesh.SmoothMeshLines('all', mesh_res, 1.4)

# ==========================================
# 4. EXCITATION & NF2FF
# ==========================================
start = [start_x + ifa_fp, gnd_stop_y, z_gnd]
stop  = [start_x + ifa_fp + trace_w, gnd_stop_y, z_top]
port = FDTD.AddLumpedPort(1, 50, start, stop, 'z', 1.0, priority=5, edges2grid='xy')

nf2ff = FDTD.CreateNF2FFBox()

CSX.Write2XML('geometry_check.xml')
CSX.Write2XML(os.path.join(Sim_Path, 'ifa_sim.xml'))

is_good = input("Is the geometry correct? (y/n): ")
if is_good.lower() != 'y':
    print("Aborting simulation. Please fix the geometry and try again.")
    exit(0)

print(f"Running FDTD in: {Sim_Path}")
FDTD.Run(Sim_Path, cleanup=True)

# ==========================================
# 5. POST-PROCESSING
# ==========================================
print("\nCalculating S-Parameters...")
f = np.linspace(max(1e9, f0-fc), f0+fc, 401)
port.CalcPort(Sim_Path, f)
s11 = port.uf_ref / port.uf_inc
s11_dB = 20.0 * np.log10(np.abs(s11))

fig, axis = plt.subplots(num="S11", tight_layout=True)
axis.plot(f/1e6, s11_dB, 'k-',  linewidth=2, label='S11')
axis.axvline(915, color='r', linestyle='--', label='915 MHz Target')
axis.grid()
axis.set_xmargin(0)
axis.set_xlabel('Frequency (MHz)')
axis.set_ylabel('S-Parameter (dB)')
axis.set_title("Input matching")
axis.legend()

idx = np.where(s11_dB == np.min(s11_dB))[0]
f_res = f[idx[0]]
print(f"\n--- RESULTS ---\nResonant Frequency: {f_res/1e6:.2f} MHz\nS11 at Resonance:   {s11_dB[idx[0]]:.2f} dB\n---------------")

if s11_dB[idx[0]] < -5:
    print("Exporting ParaView VTK...")
    theta = np.arange(-180.0, 180.0, 2.0)
    phi   = np.arange(-180.0, 180.0, 2.0)
    nf2ff_res = nf2ff.CalcNF2FF(Sim_Path, f_res, theta*np.pi/180.0, phi*np.pi/180.0, center=[0,0,1e-3])
    vtk_path = os.path.join(Sim_Path, '3D_Pattern.vtk')
    export_vtk_fallback(vtk_path, nf2ff_res, theta*np.pi/180.0, phi*np.pi/180.0)
    print(f"ParaView file saved successfully to: {vtk_path}")

plt.show()
