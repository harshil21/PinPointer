## EXAMPLE / antennas / inverted-f antenna (ifa) 2.4GHz
#
# This example demonstrates how to:
#  - calculate the reflection coefficient of an ifa
#  - calculate farfield of an ifa
#
# Converted from Octave/MATLAB to Python
# Original: (C) 2013 Stefan Mahr <dac922@gmx.de>

import numpy as np
import os
import matplotlib.pyplot as plt
from pylab import *

from CSXCAD import ContinuousStructure
from openEMS import openEMS
from openEMS.physical_constants import C0, EPS0

## Setup the simulation
unit = 1e-3  # all lengths in mm

########################################################################
#                substrate.width
#  _______________________________________________    __ substrate.
# | A                        ifa.l                |\  __    thickness
# | |ifa.e         __________________________     | |
# | |             |    ___  _________________| w2 | |
# | |       ifa.h |   |   ||                      | |
# |_V_____________|___|___||______________________| |
# |                .w1   .wf\                     | |
# |                   |.fp|  \                    | |
# |                       |    feed point         | |
# |                       |                       | | substrate.length
# |<- substrate.width/2 ->|                       | |
# |                                               | |
# |_______________________________________________| |
#  \_______________________________________________\|
#
# Note: It's not checked whether your settings make sense, so check
#       graphical output carefully.
#

substrate_width     = 80       # width of substrate
substrate_length    = 80       # length of substrate
substrate_thickness = 1.5      # thickness of substrate
substrate_cells     = 4        # use 4 cells for meshing substrate

ifa_h  = 8      # height of short circuit stub
ifa_l  = 22.5   # length of radiating element
ifa_w1 = 4      # width of short circuit stub
ifa_w2 = 2.5    # width of radiating element
ifa_wf = 1      # width of feed element
ifa_fp = 4      # position of feed element relative to short circuit stub
ifa_e  = 10     # distance to edge

# substrate setup
substrate_epsR  = 4.3
substrate_kappa = 1e-3 * 2 * np.pi * 2.45e9 * EPS0 * substrate_epsR

# setup feeding
feed_R = 50  # feed resistance

# open AppCSXCAD and show ifa
show = 1

########################################################################
# size of the simulation box
SimBox = np.array([substrate_width * 2, substrate_length * 2, 150])

## Setup FDTD parameter & excitation function
f0 = 2.5e9   # center frequency
fc = 1e9     # 20 dB corner frequency

FDTD = openEMS(NrTS=60000)
FDTD.SetGaussExcite(f0, fc)
FDTD.SetBoundaryCond(['MUR', 'MUR', 'MUR', 'MUR', 'MUR', 'MUR'])

## Setup CSXCAD geometry & mesh
CSX = ContinuousStructure()
FDTD.SetCSX(CSX)
mesh = CSX.GetGrid()
mesh.SetDeltaUnit(unit)

# initialize the mesh with the "air-box" dimensions
mesh.AddLine('x', [-SimBox[0] / 2, SimBox[0] / 2])
mesh.AddLine('y', [-SimBox[1] / 2, SimBox[1] / 2])
mesh.AddLine('z', [-SimBox[2] / 2, SimBox[2] / 2])

## Create substrate
substrate = CSX.AddMaterial('substrate', epsilon=substrate_epsR, kappa=substrate_kappa)
start = [-substrate_width / 2,  -substrate_length / 2,                   0]
stop  = [ substrate_width / 2,   substrate_length / 2,  substrate_thickness]
substrate.AddBox(start, stop, priority=1)

# add extra cells to discretize the substrate thickness
mesh.AddLine('z', np.linspace(0, substrate_thickness, substrate_cells + 1))

## Create ground plane
groundplane = CSX.AddMetal('groundplane')  # perfect electric conductor (PEC)
start = [-substrate_width / 2,  -substrate_length / 2,       substrate_thickness]
stop  = [ substrate_width / 2,   substrate_length / 2 - ifa_e, substrate_thickness]
groundplane.AddBox(start, stop, priority=10)

## Create ifa
ifa = CSX.AddMetal('ifa')  # perfect electric conductor (PEC)
tl = np.array([0, substrate_length / 2 - ifa_e, substrate_thickness])  # translate

# feed element
start = np.array([0,    0.5, 0]) + tl
stop  = start + np.array([ifa_wf, ifa_h - 0.5, 0])
ifa.AddBox(start.tolist(), stop.tolist(), priority=10)

# short circuit stub
start = np.array([-ifa_fp, 0, 0]) + tl
stop  = start + np.array([-ifa_w1, ifa_h, 0])
ifa.AddBox(start.tolist(), stop.tolist(), priority=10)

# radiating element
start = np.array([(-ifa_fp - ifa_w1), ifa_h, 0]) + tl
stop  = start + np.array([ifa_l, -ifa_w2, 0])
ifa.AddBox(start.tolist(), stop.tolist(), priority=10)

# Manually collect IFA edge coordinates (replaces MATLAB's DetectEdges)
# and smooth them into the mesh using the 1/3 rule (see wiki.openems.de/index.php/FDTD_Mesh.html)
from CSXCAD.SmoothMeshLines import SmoothMeshLines

mres = 0.5  # metal mesh resolution (mm)

# --- 1/3 Rule ---
# At each metal edge, place one mesh line 1/3*mres INSIDE and one 2/3*mres OUTSIDE
# the metal boundary, rather than exactly on the edge.
# This reduces field-enhancement errors at PEC corners in FDTD.
#
# Helper: given a list of (edge, metal_direction) pairs, return thirds-offset lines.
#   metal_dir = +1 means metal extends in the positive direction from the edge
#   metal_dir = -1 means metal extends in the negative direction from the edge
def thirds(edge, metal_dir, res=mres):
    """Return the two mesh lines for the 1/3 rule at a metal edge."""
    inside  =  metal_dir * res / 3        # 1/3 res into the metal
    outside = -metal_dir * 2 * res / 3   # 2/3 res out of the metal
    return [edge + inside, edge + outside]

# X-axis edges (metal_dir: +1 = metal goes right/+x, -1 = metal goes left/-x)
ifa_thirds_x = (
    thirds(tl[0],                            +1) +   # feed element: left edge,  metal goes right
    thirds(tl[0] + ifa_wf,                   -1) +   # feed element: right edge, metal goes left
    thirds(tl[0] - ifa_fp,                   -1) +   # short circuit stub: right edge, metal goes left
    thirds(tl[0] - ifa_fp - ifa_w1,          +1) +   # short circuit stub: left edge,  metal goes right
    thirds(tl[0] - ifa_fp - ifa_w1 + ifa_l,  -1)     # radiating element: far right edge, metal goes left
)

# Y-axis edges (metal_dir: +1 = metal goes up/+y, -1 = metal goes down/-y)
ifa_thirds_y = (
    thirds(tl[1],                  +1) +   # base of stubs, metal goes up
    thirds(tl[1] + 0.5,            +1) +   # feed element bottom, metal goes up
    thirds(tl[1] + ifa_h - 0.5,   -1) +   # feed element top,    metal goes down
    thirds(tl[1] + ifa_h,         -1) +   # top of stubs,         metal goes down
    thirds(tl[1] + ifa_h - ifa_w2, +1)    # radiating element lower edge, metal goes up
)

mesh.AddLine('x', SmoothMeshLines(ifa_thirds_x, mres))
mesh.AddLine('y', SmoothMeshLines(ifa_thirds_y, mres))

# --- Old mesh lines without 1/3 rule (kept for reference) ---
# ifa_edges_x = [
#     tl[0],                               # feed element: left edge
#     tl[0] + ifa_wf,                      # feed element: right edge
#     tl[0] - ifa_fp,                      # short circuit stub: right edge
#     tl[0] - ifa_fp - ifa_w1,             # short circuit stub: left edge
#     tl[0] - ifa_fp - ifa_w1 + ifa_l,    # radiating element: far right edge
# ]
# ifa_edges_y = [
#     tl[1],                               # base of stubs
#     tl[1] + 0.5,                         # feed element: bottom
#     tl[1] + ifa_h - 0.5,                 # feed element: top
#     tl[1] + ifa_h,                       # top of short circuit stub
#     tl[1] + ifa_h - ifa_w2,             # radiating element: lower edge
# ]
# mesh.AddLine('x', SmoothMeshLines(ifa_edges_x, 0.5))
# mesh.AddLine('y', SmoothMeshLines(ifa_edges_y, 0.5))

## Apply excitation & lumped port (current source)
start = np.array([0,    0,   0]) + tl
stop  = start + np.array([ifa_wf, 0.5, 0])
port = FDTD.AddLumpedPort(port_nr=1, R=feed_R, start=start, stop=stop,
                  p_dir='y', excite=True, priority=5)

## Finalize the mesh
# generate a smooth mesh with max. cell size: lambda_min / 20
max_res = C0 / (f0 + fc) / unit / 20
mesh.SmoothMeshLines('all', max_res)

## Add NF2FF calculation box; 3 cells away from MUR boundary condition
from openEMS.nf2ff import nf2ff as NF2FF
nf2ff_lines_x = mesh.GetLines('x')
nf2ff_lines_y = mesh.GetLines('y')
nf2ff_lines_z = mesh.GetLines('z')
start_nf2ff = [nf2ff_lines_x[3],  nf2ff_lines_y[3],  nf2ff_lines_z[3]]
stop_nf2ff  = [nf2ff_lines_x[-4], nf2ff_lines_y[-4], nf2ff_lines_z[-4]]
nf2ff_box = NF2FF(CSX, 'nf2ff', start_nf2ff, stop_nf2ff)

## Prepare simulation folder
Sim_CSX  = 'IFA.xml'

Sim_Path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'tmp_IFA')
os.makedirs(Sim_Path, exist_ok=True)

## Write openEMS compatible xml-file
CSX.Write2XML(os.path.join(Sim_Path, Sim_CSX))

## Show the structure
if show == 1:
    from CSXCAD import AppCSXCAD_BIN
    os.system(AppCSXCAD_BIN + ' "{}"'.format(os.path.join(Sim_Path, Sim_CSX)))

## Run openEMS
FDTD.Run(Sim_Path, cleanup=False)

## Postprocessing & plots
freq = np.linspace(max(1e9, f0 - fc), f0 + fc, 501)
port.CalcPort(Sim_Path, freq)

Zin = port.uf_tot / port.if_tot
s11 = port.uf_ref / port.uf_inc
P_in = 0.5 * port.uf_tot * np.conj(port.if_tot)  # antenna feed power

# plot feed point impedance
figure()
plot(freq / 1e6, np.real(Zin), 'k-',  linewidth=2)
plot(freq / 1e6, np.imag(Zin), 'r--', linewidth=2)
grid(True)
title('feed point impedance')
xlabel('frequency f / MHz')
ylabel(r'impedance $Z_{in}$ / Ohm')
legend(['real', 'imag'])

# plot reflection coefficient S11
figure()
plot(freq / 1e6, 20 * np.log10(np.abs(s11)), 'k-', linewidth=2)
grid(True)
title(r'reflection coefficient $S_{11}$')
xlabel('frequency f / MHz')
ylabel(r'reflection coefficient $|S_{11}|$')

plt.draw()
plt.pause(0.001)

## NF2FF contour plots
# find resonance frequency from s11
f_res_ind = np.argmin(np.abs(s11))
f_res = freq[f_res_ind]

print('calculating 3D far field pattern and dumping to vtk (use Paraview to visualize)...')
thetaRange = np.arange(0, 181, 2)
phiRange   = np.arange(0, 361, 2) - 180

nf2ff_res = nf2ff_box.CalcNF2FF(
    Sim_Path, f_res,
    thetaRange * np.pi / 180,
    phiRange   * np.pi / 180,
    verbose=1,
    outfile='3D_Pattern.h5'
)

# nf2ff_box.plotFF3D(nf2ff_res)

# display power and directivity
# print(f'radiated power: Prad = {nf2ff_res.Prad} Watt')
# print(f'directivity: Dmax = {nf2ff_res.Dmax} ({10 * np.log10(nf2ff_res.Dmax):.2f} dBi)')
# print(f'efficiency: nu_rad = {100 * nf2ff_res.Prad / np.real(P_in[f_res_ind]):.2f} %')

# E_far_normalized = (nf2ff_res.E_norm[0] / np.max(np.abs(nf2ff_res.E_norm[0]))
#                     * nf2ff_res.Dmax)
# nf2ff_box.DumpFF2VTK(
#     os.path.join(Sim_Path, '3D_Pattern.vtk'),
#     E_far_normalized,
#     thetaRange,
#     phiRange,
#     scale=1e-3
# )

plt.show()