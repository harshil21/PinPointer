"""
915 MHz Meandered Inverted-F Antenna — openEMS FDTD simulation.

PARAVIEW WORKFLOW (read before opening Paraview)
────────────────────────────────────────────────
All exported files share the same coordinate system (millimetres).
The far-field sphere has radius = FAR_FIELD_SPHERE_RADIUS_MM so it
wraps around the field dump region.

  Et_*.vtr            — time-domain E-field snapshots (animated)
  far_field.vtk       — radiation pattern sphere, colored by directivity
  pcb_with_antenna.vtk — board + IFA traces for orientation reference

STEP 1 — Load and animate E-field:
  File → Open → select ALL Et_*.vtr files at once → open as group/series.
  In Properties panel click Apply.
  In the toolbar, change the color field from "Solid Color" to "E" (vector).
  Apply Filters → Common → Calculator:
    Result Array Name: E_magnitude
    Expression: mag(E)
  Click Apply. Color by E_magnitude.
  Press Play (▶) — the wave will animate outward from the feed point.

STEP 2 — Add radiation pattern:
  File → Open → far_field.vtk → Apply.
  Color by "directivity_dBi".
  Set Opacity to 0.5 so you can see the field animation through it.

STEP 3 — Add PCB for orientation:
  File → Open → pcb_with_antenna.vtk → Apply.
  Set representation to "Surface With Edges", opacity 0.8.
  The board sits at Z=0 to Z=1.6. The antenna traces are visible as
  raised lines at Z=1.6. The far-field sphere and field dump are
  centred at the same origin so everything aligns without rescaling.

WHY TWO OPPOSITE BLUE (LOW) REGIONS ON THE SPHERE:
  The IFA arm runs along the Y axis. Radiation is strongest broadside
  (perpendicular) to the arm. End-fire along ±Y produces two symmetrical
  nulls — this is correct physics, identical to a dipole pattern.
  It does NOT mean the antenna is bad. The XY azimuth cut shows the
  coverage in the horizontal plane where the device will operate.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from CSXCAD import ContinuousStructure
from CSXCAD.SmoothMeshLines import SmoothMeshLines
from openEMS import openEMS
from openEMS.physical_constants import C0, EPS0

# Radius of the exported far-field sphere in mm.
# Set to wrap comfortably around the field dump box.
FAR_FIELD_SPHERE_RADIUS_MM = 150.0


# ─────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────


@dataclass
class AntennaConfig:
    # Frequency
    center_frequency_hz: float = 915e6
    bandwidth_hz: float = 100e6

    # Board & substrate
    board_width_mm: float = 42.12
    board_length_mm: float = 65.54
    board_thickness_mm: float = 1.6
    substrate_epsr: float = 4.6
    substrate_loss_tangent: float = 0.015

    # Keepout strip (left edge — no GND copper)
    keepout_width_mm: float = 13.18
    top_margin_mm: float = 2.0
    bottom_margin_mm: float = 1.0

    # Stub dimensions
    sc_stub_length_mm: float = 12.0
    sc_trace_width_mm: float = 1.0
    feed_trace_width_mm: float = 0.5
    feed_separation_mm: float = 0.3   # ← IMPEDANCE TUNE
    arm_trace_width_mm: float = 1.0
    port_width_mm: float = 0.5

    # Meander arm
    n_meanders: int = 2               # ← PRIMARY FREQUENCY TUNE
    init_length_mm: float = 10.0
    meander_width_mm: float = 7.0
    meander_v_gap_mm: float = 4.0     # ← MEDIUM FREQUENCY TUNE
    meander_h_gap_mm: float = 4.0
    tail_length_mm: float = 16.3      # ← FINE FREQUENCY TUNE

    # FDTD solver
    max_timesteps: int = 120_000
    max_time_s: float = 150.0
    end_criteria: float = 1e-4
    # Record one E-field snapshot every N timesteps.
    # 500 → ~240 frames for a 120k step run, manageable file count.
    field_dump_every_n_steps: int = 500

    # ── Derived geometry ─────────────────────────────────────────────

    @property
    def substrate_kappa(self) -> float:
        return (
            self.substrate_loss_tangent
            * 2.0 * np.pi * self.center_frequency_hz
            * EPS0 * self.substrate_epsr
        )

    @property
    def gnd_x_mm(self) -> float:
        return -self.board_width_mm / 2.0 + self.keepout_width_mm

    @property
    def port_left_x_mm(self) -> float:
        return self.gnd_x_mm - self.port_width_mm

    @property
    def arm_spine_x_mm(self) -> float:
        return self.gnd_x_mm - self.sc_stub_length_mm

    @property
    def arm_spine_right_x_mm(self) -> float:
        return self.arm_spine_x_mm + self.arm_trace_width_mm

    @property
    def meander_right_x_mm(self) -> float:
        return self.arm_spine_x_mm + self.meander_width_mm

    @property
    def sc_stub_top_y_mm(self) -> float:
        return self.board_length_mm / 2.0 - self.top_margin_mm

    @property
    def sc_stub_bot_y_mm(self) -> float:
        return self.sc_stub_top_y_mm - self.sc_trace_width_mm

    @property
    def arm_top_y_mm(self) -> float:
        return self.sc_stub_bot_y_mm

    @property
    def feed_stub_top_y_mm(self) -> float:
        return self.arm_top_y_mm - self.feed_separation_mm

    @property
    def feed_stub_bot_y_mm(self) -> float:
        return self.feed_stub_top_y_mm - self.feed_trace_width_mm

    @property
    def mesh_resolution_mm(self) -> float:
        return C0 / (self.center_frequency_hz + self.bandwidth_hz) / 1e-3 / 20

    def estimated_arm_length_mm(self) -> float:
        unique_horizontal = self.meander_width_mm - self.arm_trace_width_mm
        return (
            self.init_length_mm
            + self.n_meanders * (2.0 * unique_horizontal + self.meander_v_gap_mm)
            + max(0, self.n_meanders - 1) * self.meander_h_gap_mm
            + self.tail_length_mm
        )

    def validate(self) -> None:
        assert self.meander_right_x_mm < self.gnd_x_mm, (
            f"Meander right {self.meander_right_x_mm:.2f} overlaps GND {self.gnd_x_mm:.2f}"
        )
        assert (
            self.feed_separation_mm + self.feed_trace_width_mm <= self.init_length_mm
        ), "Feed stub overlaps first meander — increase init_length_mm"


# ─────────────────────────────────────────────────────────────────────
# GEOMETRY BUILDERS
# ─────────────────────────────────────────────────────────────────────


def add_substrate(csx: ContinuousStructure, cfg: AntennaConfig) -> None:
    mat = csx.AddMaterial("substrate", epsilon=cfg.substrate_epsr, kappa=cfg.substrate_kappa)
    mat.AddBox(
        [-cfg.board_width_mm / 2, -cfg.board_length_mm / 2, 0],
        [cfg.board_width_mm / 2, cfg.board_length_mm / 2, cfg.board_thickness_mm],
        priority=1,
    )


def add_ground_plane(csx: ContinuousStructure, cfg: AntennaConfig) -> None:
    gnd = csx.AddMetal("ground_plane")
    gnd.AddBox(
        [cfg.gnd_x_mm, -cfg.board_length_mm / 2, cfg.board_thickness_mm],
        [cfg.board_width_mm / 2, cfg.board_length_mm / 2, cfg.board_thickness_mm],
        priority=10,
    )


def add_ifa_traces(
    csx: ContinuousStructure, cfg: AntennaConfig,
) -> tuple[list[float], list[float], float]:
    """Build IFA metal traces. Returns (edge_x, edge_y, arm_bottom_y)."""
    ifa = csx.AddMetal("ifa")
    edge_x: list[float] = [cfg.gnd_x_mm]
    edge_y: list[float] = []

    # Collect trace geometry for mesh AND for VTK export later
    def add_trace(x0: float, y0: float, x1: float, y1: float) -> None:
        ifa.AddBox(
            [min(x0, x1), min(y0, y1), cfg.board_thickness_mm],
            [max(x0, x1), max(y0, y1), cfg.board_thickness_mm],
            priority=10,
        )
        edge_x.extend([x0, x1])
        edge_y.extend([y0, y1])

    add_trace(cfg.arm_spine_x_mm, cfg.sc_stub_bot_y_mm, cfg.gnd_x_mm, cfg.sc_stub_top_y_mm)
    add_trace(cfg.arm_spine_x_mm, cfg.feed_stub_bot_y_mm, cfg.port_left_x_mm, cfg.feed_stub_top_y_mm)

    current_y = cfg.arm_top_y_mm
    add_trace(cfg.arm_spine_x_mm, current_y - cfg.init_length_mm, cfg.arm_spine_right_x_mm, current_y)
    current_y -= cfg.init_length_mm

    for meander_idx in range(cfg.n_meanders):
        top_bar_bot = current_y - cfg.arm_trace_width_mm
        gap_bot = top_bar_bot - cfg.meander_v_gap_mm
        bot_bar_bot = gap_bot - cfg.arm_trace_width_mm
        right_col_left = cfg.meander_right_x_mm - cfg.arm_trace_width_mm

        add_trace(cfg.arm_spine_x_mm, top_bar_bot, cfg.meander_right_x_mm, current_y)
        add_trace(right_col_left, gap_bot, cfg.meander_right_x_mm, top_bar_bot)
        add_trace(cfg.arm_spine_x_mm, bot_bar_bot, cfg.meander_right_x_mm, gap_bot)
        current_y = bot_bar_bot

        if meander_idx < cfg.n_meanders - 1:
            add_trace(cfg.arm_spine_x_mm, current_y - cfg.meander_h_gap_mm,
                      cfg.arm_spine_right_x_mm, current_y)
            current_y -= cfg.meander_h_gap_mm

    add_trace(cfg.arm_spine_x_mm, current_y - cfg.tail_length_mm,
              cfg.arm_spine_right_x_mm, current_y)
    arm_bottom_y = current_y - cfg.tail_length_mm

    assert arm_bottom_y > -cfg.board_length_mm / 2 + cfg.bottom_margin_mm, (
        f"Arm bottom {arm_bottom_y:.2f} mm exits board"
    )

    return edge_x, edge_y, arm_bottom_y


def add_lumped_port(fdtd: openEMS, cfg: AntennaConfig) -> object:
    return fdtd.AddLumpedPort(
        port_nr=1, R=50,
        start=[cfg.port_left_x_mm, cfg.feed_stub_bot_y_mm, cfg.board_thickness_mm],
        stop=[cfg.gnd_x_mm, cfg.feed_stub_top_y_mm, cfg.board_thickness_mm],
        p_dir="x", excite=True, priority=5,
    )


def add_efield_dump(csx: ContinuousStructure, cfg: AntennaConfig) -> None:
    """
    Record time-domain E-field in a box covering the antenna + near field.

    The dump box is centred on the antenna region. It does NOT cover the
    full 200mm air box — that would produce enormous files.
    Files are written as Et_*.vtr (VTK rectilinear grid, one per step).

    Spatial sub-sampling [2,2,2] halves the resolution in each axis,
    reducing each file to 1/8 the size of a full-resolution dump.
    """
    dump = csx.AddDump("Et", dump_type=0, file_type=0,
                       sub_sampling=[2, 2, 2])
    dump.AddBox(
        [-cfg.board_width_mm / 2 - 10, cfg.arm_top_y_mm - 80, -15],
        [cfg.board_width_mm / 2 + 10, cfg.sc_stub_top_y_mm + 15, cfg.board_thickness_mm + 40],
    )


def build_mesh(
    mesh: object, cfg: AntennaConfig,
    edge_x: list[float], edge_y: list[float],
    port_edge_x: list[float], port_edge_y: list[float],
) -> None:
    mesh.AddLine("x", [-100, 100])
    mesh.AddLine("y", [-100, 100])
    mesh.AddLine("z", [-100, 100])
    mesh.AddLine("z", np.linspace(0, cfg.board_thickness_mm, 6))
    mesh.AddLine("x", SmoothMeshLines(sorted(set(edge_x + port_edge_x)), 0.5))
    mesh.AddLine("y", SmoothMeshLines(sorted(set(edge_y + port_edge_y)), 0.5))
    mesh.SmoothMeshLines("all", cfg.mesh_resolution_mm, 1.4)


# ─────────────────────────────────────────────────────────────────────
# SIMULATION RUNNER
# ─────────────────────────────────────────────────────────────────────


def build_and_run(
    cfg: AntennaConfig,
    sim_dir: Path,
    preview_geometry: bool = True,
    post_proc_only: bool = False,
) -> tuple[object, object]:
    cfg.validate()
    lam4 = C0 / (4.0 * cfg.center_frequency_hz) / 1e-3
    arm_est = cfg.estimated_arm_length_mm()
    print(f"  Estimated arm  : {arm_est:.1f} mm  |  λ/4 = {lam4:.1f} mm  |  vf ≈ {arm_est / lam4:.2f}")

    fdtd = openEMS(
        NrTS=cfg.max_timesteps, MaxTime=cfg.max_time_s,
        CoordSystem=0, EndCriteria=cfg.end_criteria,
    )
    fdtd.SetGaussExcite(cfg.center_frequency_hz, cfg.bandwidth_hz)
    fdtd.SetBoundaryCond(["MUR"] * 6)

    csx = ContinuousStructure()
    fdtd.SetCSX(csx)
    mesh = csx.GetGrid()
    mesh.SetDeltaUnit(1e-3)

    add_substrate(csx, cfg)
    add_ground_plane(csx, cfg)
    edge_x, edge_y, arm_bottom = add_ifa_traces(csx, cfg)
    port = add_lumped_port(fdtd, cfg)
    add_efield_dump(csx, cfg)

    port_ex = [cfg.port_left_x_mm, cfg.gnd_x_mm]
    port_ey = [cfg.feed_stub_bot_y_mm, cfg.feed_stub_top_y_mm]
    build_mesh(mesh, cfg, edge_x, edge_y, port_ex, port_ey)
    nf2ff = fdtd.CreateNF2FFBox()

    nx, ny, nz = [len(mesh.GetLines(d)) for d in "xyz"]
    print(f"  Mesh  X={nx} Y={ny} Z={nz}  cells≈{(nx-1)*(ny-1)*(nz-1):,}")

    sim_dir.mkdir(parents=True, exist_ok=True)
    xml_path = sim_dir / "ifa_915.xml"
    csx.Write2XML(str(xml_path))

    print(f"  SC stub  y=[{cfg.sc_stub_bot_y_mm:.2f},{cfg.sc_stub_top_y_mm:.2f}]"
          f"  x=[{cfg.arm_spine_x_mm:.2f},{cfg.gnd_x_mm:.2f}]")
    print(f"  Feed     y=[{cfg.feed_stub_bot_y_mm:.2f},{cfg.feed_stub_top_y_mm:.2f}]"
          f"  x=[{cfg.arm_spine_x_mm:.2f},{cfg.port_left_x_mm:.2f}]")
    print(f"  Arm bottom y={arm_bottom:.2f} mm")

    if preview_geometry:
        try:
            subprocess.Popen(["AppCSXCAD", str(xml_path)]).wait()
        except FileNotFoundError:
            pass

    input("\nPress [ENTER] to run FDTD, Ctrl+C to abort.\n")
    if not post_proc_only:
        fdtd.Run(str(sim_dir), cleanup=True)

    return port, nf2ff


# ─────────────────────────────────────────────────────────────────────
# POST-PROCESSING
# ─────────────────────────────────────────────────────────────────────


def compute_s_parameters(
    port: object, sim_dir: Path, cfg: AntennaConfig, n_freq_points: int = 501,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    freq = np.linspace(800e6, cfg.center_frequency_hz + cfg.bandwidth_hz, n_freq_points)
    port.CalcPort(str(sim_dir), freq)
    s11 = port.uf_ref / port.uf_inc
    s11_db = 20.0 * np.log10(np.abs(s11) + 1e-30)
    zin = port.uf_tot / port.if_tot
    return freq, s11_db, np.real(zin), np.imag(zin)


def compute_far_field(
    nf2ff: object, sim_dir: Path, cfg: AntennaConfig, resonant_frequency_hz: float,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, float]:
    theta_deg = np.arange(0.0, 181.0, 2.0)
    phi_deg = np.arange(0.0, 360.0, 5.0)
    result = nf2ff.CalcNF2FF(
        str(sim_dir), resonant_frequency_hz, theta_deg, phi_deg,
        center=[0.0, 0.0, cfg.board_thickness_mm * 1e-3],
        read_cached=True, outfile="nf2ff_result.h5",
    )
    e_norm = result.E_norm[0]
    dmax = result.Dmax[0]
    directivity_dbi = 10.0 * np.log10(dmax * (e_norm / np.max(e_norm)) ** 2 + 1e-30)
    return theta_deg, phi_deg, directivity_dbi, 10.0 * np.log10(dmax)


# ─────────────────────────────────────────────────────────────────────
# PLOTS
# ─────────────────────────────────────────────────────────────────────


def plot_s11_and_impedance(
    freq: np.ndarray, s11_db: np.ndarray,
    re_zin: np.ndarray, im_zin: np.ndarray,
    cfg: AntennaConfig, output_path: Path,
) -> None:
    idx = int(np.argmin(s11_db))
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)
    ax1.plot(freq / 1e6, s11_db, "royalblue", lw=2, label="S11")
    ax1.axvline(915, color="crimson", ls="--", lw=1.5, label="915 MHz")
    ax1.axhline(-10, color="gray", ls=":", lw=1)
    ax1.axhline(-15, color="green", ls=":", lw=1, label="−15 dB goal")
    ax1.scatter(freq[idx] / 1e6, s11_db[idx], color="crimson", zorder=5,
                label=f"{freq[idx] / 1e6:.0f} MHz, {s11_db[idx]:.1f} dB")
    ax1.set(xlabel="Frequency (MHz)", ylabel="S11 (dB)",
            title=f"S11  N={cfg.n_meanders}  V_gap={cfg.meander_v_gap_mm} mm"
                  f"  feed_sep={cfg.feed_separation_mm} mm",
            xlim=[freq[0] / 1e6, freq[-1] / 1e6], ylim=[-40, 5])
    ax1.grid(True, alpha=0.35); ax1.legend(fontsize=8)

    ax2.plot(freq / 1e6, re_zin, "k-", lw=2, label="Re{Zin}")
    ax2.plot(freq / 1e6, im_zin, "r--", lw=2, label="Im{Zin}")
    ax2.axvline(915, color="royalblue", ls="--", lw=1.5)
    ax2.axhline(50, color="green", ls=":", lw=1.2, label="50 Ω")
    ax2.axhline(0, color="gray", ls="-", lw=0.8)
    ax2.axvline(freq[idx] / 1e6, color="crimson", ls=":", lw=1.5,
                label=f"S11 min @ {freq[idx] / 1e6:.0f} MHz  Re={re_zin[idx]:.0f} Ω")
    ax2.set(xlabel="Frequency (MHz)", ylabel="Impedance (Ω)",
            xlim=[freq[0] / 1e6, freq[-1] / 1e6], ylim=[-200, 300])
    ax2.grid(True, alpha=0.35); ax2.legend(fontsize=8)
    plt.savefig(output_path, dpi=150, bbox_inches="tight"); plt.show()


def plot_far_field_cuts(
    theta_deg: np.ndarray, phi_deg: np.ndarray,
    directivity_dbi: np.ndarray, dmax_dbi: float,
    resonant_frequency_hz: float, output_path: Path,
    db_floor: float = -20.0,
) -> None:
    """
    360° elevation cuts (XZ, YZ) and full azimuth cut (XY).

    Each elevation cut concatenates the forward half (phi=0° or 90°,
    theta 0→180) with the reversed back half (phi+180°, theta 180→0)
    so the polar plot forms a closed circle.
    """
    def make_elevation_cut(phi_idx_fwd: int, phi_idx_back: int) -> tuple[np.ndarray, np.ndarray]:
        fwd = directivity_dbi[:, phi_idx_fwd]
        back = directivity_dbi[::-1, phi_idx_back]
        angles = np.linspace(0, 2 * np.pi, len(fwd) + len(back), endpoint=False)
        pattern = np.concatenate([fwd, back])
        return angles, np.clip(pattern, db_floor, None) - db_floor

    def make_azimuth_cut(theta_idx: int) -> tuple[np.ndarray, np.ndarray]:
        pattern = directivity_dbi[theta_idx, :]
        return np.deg2rad(phi_deg), np.clip(pattern, db_floor, None) - db_floor

    idx = {
        "phi_0":    int(np.argmin(np.abs(phi_deg - 0.0))),
        "phi_90":   int(np.argmin(np.abs(phi_deg - 90.0))),
        "phi_180":  int(np.argmin(np.abs(phi_deg - 180.0))),
        "phi_270":  int(np.argmin(np.abs(phi_deg - 270.0))),
        "theta_90": int(np.argmin(np.abs(theta_deg - 90.0))),
    }

    fig, axes = plt.subplots(1, 3, subplot_kw={"projection": "polar"}, figsize=(15, 5))
    fig.suptitle(
        f"Far-Field — {resonant_frequency_hz / 1e6:.0f} MHz  Dmax = {dmax_dbi:.1f} dBi",
        fontsize=13,
    )

    plots = [
        (axes[0], *make_elevation_cut(idx["phi_0"], idx["phi_180"]),
         "XZ elevation  (φ=0°/180°)"),
        (axes[1], *make_elevation_cut(idx["phi_90"], idx["phi_270"]),
         "YZ elevation  (φ=90°/270°)"),
        (axes[2], *make_azimuth_cut(idx["theta_90"]),
         "XY azimuth  (θ=90°)"),
    ]
    for ax, angles, pattern, title in plots:
        ax.plot(angles, pattern, "royalblue", lw=2)
        ax.set_title(title, pad=12, fontsize=10)
        ax.set_theta_zero_location("N")
        ax.set_theta_direction(-1)
        ax.set_rlabel_position(45)
        ticks = ax.get_yticks()
        ax.set_yticklabels([f"{v + db_floor:.0f} dBi" for v in ticks], fontsize=7)
        ax.grid(True, alpha=0.4)

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches="tight"); plt.show()


# ─────────────────────────────────────────────────────────────────────
# VTK EXPORTS
# ─────────────────────────────────────────────────────────────────────


def export_far_field_to_vtk(
    theta_deg: np.ndarray, phi_deg: np.ndarray,
    directivity_dbi: np.ndarray, output_path: Path,
) -> None:
    """
    Export radiation pattern as a CLOSED sphere at FAR_FIELD_SPHERE_RADIUS_MM.

    The sphere is a unit sphere scaled to FAR_FIELD_SPHERE_RADIUS_MM so it
    sits in the same mm coordinate system as the PCB and field dumps.

    The phi dimension is closed by appending phi=0° as phi=360° — this
    removes the seam/break that appears in open StructuredGrids.

    In Paraview: color by "directivity_dBi". Do NOT warp. Opacity ~0.5.
    """
    r = FAR_FIELD_SPHERE_RADIUS_MM

    # Close phi: repeat first column at end so the sphere has no seam
    phi_closed_deg = np.append(phi_deg, phi_deg[0] + 360.0)
    n_theta = len(theta_deg)
    n_phi_closed = len(phi_closed_deg)

    theta_grid, phi_grid = np.meshgrid(
        np.deg2rad(theta_deg), np.deg2rad(phi_closed_deg), indexing="ij"
    )
    x_pts = (r * np.sin(theta_grid) * np.cos(phi_grid)).ravel()
    y_pts = (r * np.sin(theta_grid) * np.sin(phi_grid)).ravel()
    z_pts = (r * np.cos(theta_grid)).ravel()

    # directivity values: extend first phi column to close the sphere
    dir_closed = np.hstack([directivity_dbi, directivity_dbi[:, :1]])
    dbi_flat = dir_closed.ravel()
    n_points = n_theta * n_phi_closed

    with open(output_path, "w") as f:
        f.write("# vtk DataFile Version 3.0\nIFA Far-Field\nASCII\n")
        f.write("DATASET STRUCTURED_GRID\n")
        # DIMENSIONS: phi (fastest), theta (slowest), depth=1
        f.write(f"DIMENSIONS {n_phi_closed} {n_theta} 1\n")
        f.write(f"POINTS {n_points} float\n")
        for xi, yi, zi in zip(x_pts, y_pts, z_pts):
            f.write(f"{xi:.4f} {yi:.4f} {zi:.4f}\n")
        f.write(f"\nPOINT_DATA {n_points}\n")
        f.write("SCALARS directivity_dBi float 1\nLOOKUP_TABLE default\n")
        for v in dbi_flat:
            f.write(f"{v:.4f}\n")
    print(f"  Far-field VTK  → {output_path}  (radius = {r} mm, seam closed)")


def export_pcb_to_vtk(cfg: AntennaConfig, trace_boxes: list, output_path: Path) -> None:
    """
    Export PCB board outline + GND plane + IFA traces as VTK POLYDATA.

    All geometry is in mm, matching the field dump and far-field sphere
    coordinate systems. No scaling is needed in Paraview.

    The antenna traces are drawn as thin quads at z=board_thickness so
    you can see exactly where the IFA arm sits relative to the board edge.
    The GND plane region is drawn as a separate grey quad.
    """
    polys: list[list[tuple[float, float, float]]] = []
    bt = cfg.board_thickness_mm
    bw, bl = cfg.board_width_mm, cfg.board_length_mm

    # Board substrate outline (thin slab — just top face for clarity)
    bx0, bx1 = -bw / 2, bw / 2
    by0, by1 = -bl / 2, bl / 2
    polys.append([(bx0, by0, 0), (bx1, by0, 0), (bx1, by1, 0), (bx0, by1, 0)])

    # GND plane (slightly raised so it's visible over substrate)
    gx0 = cfg.gnd_x_mm
    polys.append([(gx0, by0, bt), (bx1, by0, bt), (bx1, by1, bt), (gx0, by1, bt)])

    # IFA traces — each trace_box is (x0, y0, x1, y1) at z=bt
    for x0, y0, x1, y1 in trace_boxes:
        polys.append([(x0, y0, bt), (x1, y0, bt), (x1, y1, bt), (x0, y1, bt)])

    # Flatten to a point list, deduplicate
    all_pts: list[tuple[float, float, float]] = []
    poly_indices: list[list[int]] = []
    pt_map: dict[tuple, int] = {}

    def get_pt_idx(pt: tuple) -> int:
        key = (round(pt[0], 4), round(pt[1], 4), round(pt[2], 4))
        if key not in pt_map:
            pt_map[key] = len(all_pts)
            all_pts.append(key)
        return pt_map[key]

    for poly in polys:
        poly_indices.append([get_pt_idx(p) for p in poly])

    n_pts = len(all_pts)
    n_polys = len(poly_indices)
    cell_list_size = sum(len(p) + 1 for p in poly_indices)

    with open(output_path, "w") as f:
        f.write("# vtk DataFile Version 3.0\nPCB with IFA\nASCII\nDATASET POLYDATA\n")
        f.write(f"POINTS {n_pts} float\n")
        for pt in all_pts:
            f.write(f"{pt[0]:.4f} {pt[1]:.4f} {pt[2]:.4f}\n")
        f.write(f"\nPOLYGONS {n_polys} {cell_list_size}\n")
        for pidx, poly in enumerate(poly_indices):
            f.write(f"{len(poly)} {' '.join(str(i) for i in poly)}\n")

        # Color each polygon: 0=substrate, 1=GND, 2+=IFA traces
        f.write(f"\nCELL_DATA {n_polys}\n")
        f.write("SCALARS region int 1\nLOOKUP_TABLE default\n")
        for i in range(n_polys):
            region = 0 if i == 0 else (1 if i == 1 else 2)
            f.write(f"{region}\n")

    print(f"  PCB+traces VTK → {output_path}  (all coordinates in mm, no scaling needed)")


# ─────────────────────────────────────────────────────────────────────
# ENTRY POINT
# ─────────────────────────────────────────────────────────────────────


def main() -> None:
    cfg = AntennaConfig()
    sim_dir = Path(__file__).parent / "manual_sim"

    port, nf2ff = build_and_run(cfg, sim_dir)

    freq, s11_db, re_zin, im_zin = compute_s_parameters(port, sim_dir, cfg)
    idx_min = int(np.argmin(s11_db))
    f_res = freq[idx_min]
    print(f"\n  S11 min : {f_res / 1e6:.0f} MHz  {s11_db[idx_min]:.1f} dB"
          f"  Re={re_zin[idx_min]:.0f} Ω  Im={im_zin[idx_min]:.0f} Ω")

    plot_s11_and_impedance(freq, s11_db, re_zin, im_zin, cfg, sim_dir / "s11_impedance.png")

    if s11_db[idx_min] < -10.0:
        theta, phi, dir_dbi, dmax_dbi = compute_far_field(nf2ff, sim_dir, cfg, f_res)
        print(f"  Dmax : {dmax_dbi:.1f} dBi")
        plot_far_field_cuts(theta, phi, dir_dbi, dmax_dbi, f_res, sim_dir / "far_field_cuts.png")

        export_far_field_to_vtk(theta, phi, dir_dbi, sim_dir / "far_field.vtk")
        trace_boxes = _derive_trace_boxes(cfg)
        export_pcb_to_vtk(cfg, trace_boxes, sim_dir / "pcb_with_antenna.vtk")

        print("\n  Paraview: open Et_*.vtr (time series) + far_field.vtk + pcb_with_antenna.vtk")
        print("  All files are in mm — no rescaling needed.")
        print("  For E-field animation: Filters → Calculator → Expression: mag(E) → Play ▶")
    else:
        print("  S11 > −10 dB — skipping far-field")


def _derive_trace_boxes(cfg: AntennaConfig) -> list[tuple[float, float, float, float]]:
    """Re-derive the IFA trace bounding boxes from config for VTK export."""
    boxes: list[tuple[float, float, float, float]] = []

    def record(x0: float, y0: float, x1: float, y1: float) -> None:
        boxes.append((min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1)))

    record(cfg.arm_spine_x_mm, cfg.sc_stub_bot_y_mm, cfg.gnd_x_mm, cfg.sc_stub_top_y_mm)
    record(cfg.arm_spine_x_mm, cfg.feed_stub_bot_y_mm, cfg.port_left_x_mm, cfg.feed_stub_top_y_mm)

    current_y = cfg.arm_top_y_mm
    record(cfg.arm_spine_x_mm, current_y - cfg.init_length_mm, cfg.arm_spine_right_x_mm, current_y)
    current_y -= cfg.init_length_mm

    for meander_idx in range(cfg.n_meanders):
        top_bar_bot = current_y - cfg.arm_trace_width_mm
        gap_bot = top_bar_bot - cfg.meander_v_gap_mm
        bot_bar_bot = gap_bot - cfg.arm_trace_width_mm
        right_col_left = cfg.meander_right_x_mm - cfg.arm_trace_width_mm

        record(cfg.arm_spine_x_mm, top_bar_bot, cfg.meander_right_x_mm, current_y)
        record(right_col_left, gap_bot, cfg.meander_right_x_mm, top_bar_bot)
        record(cfg.arm_spine_x_mm, bot_bar_bot, cfg.meander_right_x_mm, gap_bot)
        current_y = bot_bar_bot

        if meander_idx < cfg.n_meanders - 1:
            record(cfg.arm_spine_x_mm, current_y - cfg.meander_h_gap_mm,
                   cfg.arm_spine_right_x_mm, current_y)
            current_y -= cfg.meander_h_gap_mm

    record(cfg.arm_spine_x_mm, current_y - cfg.tail_length_mm,
           cfg.arm_spine_right_x_mm, current_y)
    return boxes


if __name__ == "__main__":
    main()