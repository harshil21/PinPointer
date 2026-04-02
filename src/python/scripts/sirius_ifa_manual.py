from __future__ import annotations

import subprocess
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from CSXCAD import ContinuousStructure
from CSXCAD.SmoothMeshLines import SmoothMeshLines
from openEMS import openEMS
from openEMS.physical_constants import C0, EPS0


class IFASimulation:
    # ── Frequency ──────────────────────────────────────────────────────
    CENTER_FREQUENCY_HZ = 915e6
    BANDWIDTH_HZ = 100e6

    # ── Board & substrate ──────────────────────────────────────────────
    BOARD_WIDTH_MM = 42.12
    BOARD_LENGTH_MM = 65.54
    BOARD_THICKNESS_MM = 1.6
    SUBSTRATE_EPSR = 4.6
    SUBSTRATE_LOSS_TANGENT = 0.015
    COPPER_EDGE_INSET_MM = 0.2  # all copper planes inset from PCB edge

    # ── Keepout strip (left edge — no GND copper) ──────────────────────
    KEEPOUT_WIDTH_MM = 13.18
    TOP_MARGIN_MM = 2.0
    BOTTOM_MARGIN_MM = 1.0

    # ── Stub dimensions ────────────────────────────────────────────────
    SC_STUB_LENGTH_MM = 12.0  # arm spine x = GND_X - this
    SC_TRACE_WIDTH_MM = 2.4
    FEED_TRACE_WIDTH_MM = 0.2
    FEED_SEPARATION_MM = 1.0  # ← IMPEDANCE TUNE
    ARM_TRACE_WIDTH_MM = 2.4
    PORT_WIDTH_MM = 0.5  # fixed — do not change

    # ── Meander arm ────────────────────────────────────────────────────
    N_MEANDERS = 0  # ← PRIMARY FREQUENCY TUNE
    INIT_LENGTH_MM = 20.0
    MEANDER_WIDTH_MM = 6.0  # must be < SC_STUB_LENGTH_MM
    MEANDER_V_GAP_MM = 6.0  # ← MEDIUM FREQUENCY TUNE
    MEANDER_H_GAP_MM = 4.0  # only used when N_MEANDERS > 1
    TAIL_LENGTH_MM = 32.5  # ← FINE FREQUENCY TUNE

    # ── Stitching vias ─────────────────────────────────────────────────
    VIA_HOLE_DIAMETER_MM = 0.3  # drill hole diameter
    VIA_OUTER_DIAMETER_MM = 0.6  # complete via diameter (hole + annular ring)
    VIA_KEEPOUT_OFFSET_MM = 0.57  # via centre to keepout edge distance

    # Via (x, y) positions in simulation coordinates [mm].
    # For vias along the keepout edge: x = GND_X_MM + VIA_KEEPOUT_OFFSET_MM
    # Fill y-values from KiCAD layout (board-centred coords).
    VIA_POSITIONS_XY_MM: list[tuple[float, float]] = [
        # Example: (-7.47, 20.0),  ← GND_X_MM + 0.57 ≈ -7.47 for default board
        # (-7.2720, -32.000),
        # (-7.2720, -30.000),
        # (-7.2720, -28.000),
        # (-7.2720, -26.000),
        # (-7.2720, -24.000),
        # (-7.2720, -22.000),
        # (-7.2720, -20.000),
        # (-7.2720, -18.000),
        # (-7.2720, -16.000),
        # (-7.2720, -14.000),
        # (-7.2720, -12.000),
        # (-7.2720, -10.000),
        # (-7.2720, -8.000),
        # (-7.2720, -6.000),
        # (-7.2720, -4.000),
        # (-7.2720, -2.000),
        # (-7.2720, -0.000),
        # (-7.2720, 2.000),
        # (-7.2720, 4.000),
        # (-7.2720, 6.000),
        # (-7.2720, 8.000),
        # (-7.2720, 10.000),
        (-7.2720, 12.000),
        (-7.2720, 14.000),
        (-7.2720, 16.000),
        (-7.2720, 18.000),
        (-7.2720, 20.000),
        (-7.2720, 22.000),
        (-7.1220, 23.160),
        (-7.2720, 28.500),
        (-7.2720, 29.500),
        (-7.2720, 30.500),
        (-7.2720, 31.500),
    ]

    # ── Solver ─────────────────────────────────────────────────────────
    MAX_TIMESTEPS = 100_000
    MAX_TIME_S = 150.0
    END_CRITERIA = 1e-4
    FIELD_DUMP_SUBSAMPLE = [2, 2, 2]  # spatial subsampling for Et dump

    # ── Export ─────────────────────────────────────────────────────────
    FAR_FIELD_RADIUS_MM = 150.0

    # ──────────────────────────────────────────────────────────────────

    def __init__(self, sim_dir: Path) -> None:
        self.sim_dir = sim_dir
        sim_dir.mkdir(parents=True, exist_ok=True)

        # Derived geometry (computed once from class constants)
        self.SUBSTRATE_KAPPA = (
            self.SUBSTRATE_LOSS_TANGENT
            * 2.0
            * np.pi
            * self.CENTER_FREQUENCY_HZ
            * EPS0
            * self.SUBSTRATE_EPSR
        )
        self.GND_X_MM = -self.BOARD_WIDTH_MM / 2.0 + self.KEEPOUT_WIDTH_MM
        self.PORT_LEFT_X_MM = self.GND_X_MM - self.PORT_WIDTH_MM
        self.ARM_SPINE_X_MM = self.GND_X_MM - self.SC_STUB_LENGTH_MM
        self.ARM_SPINE_RIGHT_X_MM = self.ARM_SPINE_X_MM + self.ARM_TRACE_WIDTH_MM
        self.MEANDER_RIGHT_X_MM = self.ARM_SPINE_X_MM + self.MEANDER_WIDTH_MM
        self.SC_STUB_TOP_Y_MM = self.BOARD_LENGTH_MM / 2.0 - self.TOP_MARGIN_MM
        self.SC_STUB_BOT_Y_MM = self.SC_STUB_TOP_Y_MM - self.SC_TRACE_WIDTH_MM
        self.ARM_TOP_Y_MM = self.SC_STUB_BOT_Y_MM
        self.FEED_STUB_TOP_Y_MM = self.ARM_TOP_Y_MM - self.FEED_SEPARATION_MM
        self.FEED_STUB_BOT_Y_MM = self.FEED_STUB_TOP_Y_MM - self.FEED_TRACE_WIDTH_MM
        self.MESH_RES_MM = (
            C0 / (self.CENTER_FREQUENCY_HZ + self.BANDWIDTH_HZ) / 1e-3 / 20
        )

        # openEMS objects — populated by setup() and build_geometry()
        self.FDTD: openEMS | None = None
        self.CSX: ContinuousStructure | None = None
        self.mesh = None
        self.port = None
        self.nf2ff = None

        # Internal state
        self._edge_x: list[float] = []
        self._edge_y: list[float] = []
        self._trace_boxes: list[tuple[float, float, float, float]] = []
        self._arm_bottom_y: float = 0.0

        # Results (populated by compute_* methods)
        self._freq: np.ndarray | None = None
        self._s11_db: np.ndarray | None = None
        self._re_zin: np.ndarray | None = None
        self._im_zin: np.ndarray | None = None
        self._f_res: float = 0.0
        self._theta: np.ndarray | None = None
        self._phi: np.ndarray | None = None
        self._dir_dbi: np.ndarray | None = None
        self._dmax_dbi: float = 0.0

        self._validate()

        arm = self._estimated_arm_length_mm()
        lam4 = C0 / (4.0 * self.CENTER_FREQUENCY_HZ) / 1e-3
        print(f"  Arm: {arm:.1f} mm  λ/4: {lam4:.1f} mm  vf: {arm / lam4:.2f}")

    def _validate(self) -> None:
        assert self.MEANDER_RIGHT_X_MM < self.GND_X_MM, (
            f"Meander right edge {self.MEANDER_RIGHT_X_MM:.2f} overlaps "
            f"GND plane at {self.GND_X_MM:.2f}"
        )
        assert (
            self.FEED_SEPARATION_MM + self.FEED_TRACE_WIDTH_MM <= self.INIT_LENGTH_MM
        ), "Feed stub overlaps first meander — increase INIT_LENGTH_MM"

    def _estimated_arm_length_mm(self) -> float:
        unique_h = self.MEANDER_WIDTH_MM - self.ARM_TRACE_WIDTH_MM
        return (
            self.INIT_LENGTH_MM
            + self.N_MEANDERS * (2.0 * unique_h + self.MEANDER_V_GAP_MM)
            + max(0, self.N_MEANDERS - 1) * self.MEANDER_H_GAP_MM
            + self.TAIL_LENGTH_MM
        )

    # ── Setup ──────────────────────────────────────────────────────────

    def setup(self) -> None:
        self.FDTD = openEMS(
            NrTS=self.MAX_TIMESTEPS,
            MaxTime=self.MAX_TIME_S,
            CoordSystem=0,
            EndCriteria=self.END_CRITERIA,
        )
        self.FDTD.SetGaussExcite(self.CENTER_FREQUENCY_HZ, self.BANDWIDTH_HZ)
        self.FDTD.SetBoundaryCond(["MUR"] * 6)

        self.CSX = ContinuousStructure()
        self.FDTD.SetCSX(self.CSX)
        self.mesh = self.CSX.GetGrid()
        self.mesh.SetDeltaUnit(1e-3)

    # ── Geometry ───────────────────────────────────────────────────────

    def build_geometry(self) -> None:
        self._add_substrate()
        self._add_ground_plane()
        self._add_ifa_traces()
        self._add_vias()
        self._add_lumped_port()
        self._add_efield_dump()
        self._add_surface_current_dump()
        self._finalize_mesh()
        self.nf2ff = self.FDTD.CreateNF2FFBox()

        nx, ny, nz = [len(self.mesh.GetLines(d)) for d in "xyz"]
        print(f"  Mesh X={nx} Y={ny} Z={nz}  cells≈{(nx - 1) * (ny - 1) * (nz - 1):,}")

    def _add_substrate(self) -> None:
        mat = self.CSX.AddMaterial(
            "substrate", epsilon=self.SUBSTRATE_EPSR, kappa=self.SUBSTRATE_KAPPA
        )
        mat.AddBox(
            [-self.BOARD_WIDTH_MM / 2, -self.BOARD_LENGTH_MM / 2, 0],
            [
                self.BOARD_WIDTH_MM / 2,
                self.BOARD_LENGTH_MM / 2,
                self.BOARD_THICKNESS_MM,
            ],
            priority=1,
        )

    def _add_ground_plane(self) -> None:
        gnd = self.CSX.AddMetal("ground_plane")
        inset = self.COPPER_EDGE_INSET_MM
        gnd.AddBox(
            [self.GND_X_MM, -self.BOARD_LENGTH_MM / 2 + inset, self.BOARD_THICKNESS_MM],
            [
                self.BOARD_WIDTH_MM / 2 - inset,
                self.BOARD_LENGTH_MM / 2 - inset,
                self.BOARD_THICKNESS_MM,
            ],
            priority=10,
        )

    def _add_vias(self) -> None:
        """Add stitching vias as metal cylinders from z=0 to z=BOARD_THICKNESS_MM."""
        if not self.VIA_POSITIONS_XY_MM:
            return

        via_metal = self.CSX.AddMetal("stitching_vias")
        via_radius = self.VIA_OUTER_DIAMETER_MM / 2.0

        for x, y in self.VIA_POSITIONS_XY_MM:
            via_metal.AddCylinder(
                start=[x, y, 0],
                stop=[x, y, self.BOARD_THICKNESS_MM],
                radius=via_radius,
                priority=10,
            )

        print(f"  Added {len(self.VIA_POSITIONS_XY_MM)} stitching via(s)")

    def _place_trace(self, metal, x0: float, y0: float, x1: float, y1: float) -> None:
        metal.AddBox(
            [min(x0, x1), min(y0, y1), self.BOARD_THICKNESS_MM],
            [max(x0, x1), max(y0, y1), self.BOARD_THICKNESS_MM],
            priority=10,
        )
        self._edge_x.extend([x0, x1])
        self._edge_y.extend([y0, y1])
        self._trace_boxes.append((min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1)))

    def _add_ifa_traces(self) -> None:
        ifa = self.CSX.AddMetal("ifa")
        self._edge_x = [self.GND_X_MM]
        self._edge_y = []
        self._trace_boxes = []

        def T(x0: float, y0: float, x1: float, y1: float) -> None:
            self._place_trace(ifa, x0, y0, x1, y1)

        T(
            self.ARM_SPINE_X_MM,
            self.SC_STUB_BOT_Y_MM,
            self.GND_X_MM,
            self.SC_STUB_TOP_Y_MM,
        )
        T(
            self.ARM_SPINE_X_MM,
            self.FEED_STUB_BOT_Y_MM,
            self.PORT_LEFT_X_MM,
            self.FEED_STUB_TOP_Y_MM,
        )

        y = self.ARM_TOP_Y_MM
        T(self.ARM_SPINE_X_MM, y - self.INIT_LENGTH_MM, self.ARM_SPINE_RIGHT_X_MM, y)
        y -= self.INIT_LENGTH_MM

        for i in range(self.N_MEANDERS):
            top_bot = y - self.ARM_TRACE_WIDTH_MM
            gap_bot = top_bot - self.MEANDER_V_GAP_MM
            bot_bot = gap_bot - self.ARM_TRACE_WIDTH_MM
            rc_left = self.MEANDER_RIGHT_X_MM - self.ARM_TRACE_WIDTH_MM

            T(self.ARM_SPINE_X_MM, top_bot, self.MEANDER_RIGHT_X_MM, y)
            T(rc_left, gap_bot, self.MEANDER_RIGHT_X_MM, top_bot)
            T(self.ARM_SPINE_X_MM, bot_bot, self.MEANDER_RIGHT_X_MM, gap_bot)
            y = bot_bot

            if i < self.N_MEANDERS - 1:
                T(
                    self.ARM_SPINE_X_MM,
                    y - self.MEANDER_H_GAP_MM,
                    self.ARM_SPINE_RIGHT_X_MM,
                    y,
                )
                y -= self.MEANDER_H_GAP_MM

        T(self.ARM_SPINE_X_MM, y - self.TAIL_LENGTH_MM, self.ARM_SPINE_RIGHT_X_MM, y)
        self._arm_bottom_y = y - self.TAIL_LENGTH_MM

        assert self._arm_bottom_y > -self.BOARD_LENGTH_MM / 2 + self.BOTTOM_MARGIN_MM, (
            f"Arm bottom {self._arm_bottom_y:.2f} mm exits board"
        )

    def _add_lumped_port(self) -> None:
        self.port = self.FDTD.AddLumpedPort(
            port_nr=1,
            R=50,
            start=[
                self.PORT_LEFT_X_MM,
                self.FEED_STUB_BOT_Y_MM,
                self.BOARD_THICKNESS_MM,
            ],
            stop=[self.GND_X_MM, self.FEED_STUB_TOP_Y_MM, self.BOARD_THICKNESS_MM],
            p_dir="x",
            excite=True,
            priority=5,
        )
        self._edge_x.extend([self.PORT_LEFT_X_MM, self.GND_X_MM])
        self._edge_y.extend([self.FEED_STUB_BOT_Y_MM, self.FEED_STUB_TOP_Y_MM])

    def _add_efield_dump(self) -> None:
        dump = self.CSX.AddDump(
            "Et", dump_type=0, file_type=0, sub_sampling=self.FIELD_DUMP_SUBSAMPLE
        )
        dump.AddBox(
            [-self.BOARD_WIDTH_MM / 2 - 80, self._arm_bottom_y - 80, -20],
            [self.BOARD_WIDTH_MM / 2 + 80, self.SC_STUB_TOP_Y_MM + 15, 150],
        )

    def _add_surface_current_dump(self) -> None:
        # H-field just above board surface gives surface current via Js = n x H
        # Place at z = BOARD_THICKNESS + one mesh cell (~0.3mm) above the copper
        z_above = self.BOARD_THICKNESS_MM + 0.3
        dump = self.CSX.AddDump("Hsurf", dump_type=1, file_type=0)
        dump.AddBox(
            [-self.BOARD_WIDTH_MM / 2, -self.BOARD_LENGTH_MM / 2, z_above],
            [self.BOARD_WIDTH_MM / 2, self.BOARD_LENGTH_MM / 2, z_above],
        )

    def _finalize_mesh(self) -> None:
        self.mesh.AddLine("x", [-100, 100])
        self.mesh.AddLine("y", [-100, 100])
        self.mesh.AddLine("z", [-100, 100])
        self.mesh.AddLine("z", np.linspace(0, self.BOARD_THICKNESS_MM, 6))
        self.mesh.AddLine("x", SmoothMeshLines(sorted(set(self._edge_x)), 0.5))
        self.mesh.AddLine("y", SmoothMeshLines(sorted(set(self._edge_y)), 0.5))
        self.mesh.SmoothMeshLines("all", self.MESH_RES_MM, 1.4)

    # ── Run ────────────────────────────────────────────────────────────

    def preview(self) -> None:
        xml_path = self.sim_dir / "ifa_915.xml"
        self.CSX.Write2XML(str(xml_path))
        try:
            subprocess.Popen(["AppCSXCAD", str(xml_path)]).wait()
        except FileNotFoundError:
            pass

    def run(self, preview: bool = True, post_process_only: bool = False) -> None:
        self.setup()
        self.build_geometry()
        if preview:
            self.preview()
        input("\nPress [ENTER] to run FDTD, Ctrl+C to abort.\n")
        if not post_process_only:
            self.FDTD.Run(str(self.sim_dir), cleanup=True)

    # ── Post-processing ────────────────────────────────────────────────

    def compute_s_parameters(self, n_points: int = 501) -> None:
        self._freq = np.linspace(
            800e6, self.CENTER_FREQUENCY_HZ + self.BANDWIDTH_HZ, n_points
        )
        self.port.CalcPort(str(self.sim_dir), self._freq)
        s11 = self.port.uf_ref / self.port.uf_inc
        self._s11_db = 20.0 * np.log10(np.abs(s11) + 1e-30)
        zin = self.port.uf_tot / self.port.if_tot
        self._re_zin = np.real(zin)
        self._im_zin = np.imag(zin)

        idx = int(np.argmin(self._s11_db))
        self._f_res = float(self._freq[idx])
        print(
            f"  S11 min: {self._f_res / 1e6:.0f} MHz  {self._s11_db[idx]:.1f} dB"
            f"  Re={self._re_zin[idx]:.0f} Ω  Im={self._im_zin[idx]:.0f} Ω"
        )

    def compute_far_field(self) -> None:
        self._theta = np.arange(0.0, 181.0, 2.0)
        self._phi = np.arange(0.0, 360.0, 5.0)
        result = self.nf2ff.CalcNF2FF(
            str(self.sim_dir),
            self._f_res,
            self._theta,
            self._phi,
            center=[0.0, 0.0, self.BOARD_THICKNESS_MM * 1e-3],
            read_cached=True,
            outfile="nf2ff_result.h5",
        )
        e_norm = result.E_norm[0]
        dmax = result.Dmax[0]
        self._dir_dbi = 10.0 * np.log10(dmax * (e_norm / np.max(e_norm)) ** 2 + 1e-30)
        self._dmax_dbi = 10.0 * np.log10(dmax)
        print(f"  Dmax: {self._dmax_dbi:.1f} dBi")

    # ── Plots ──────────────────────────────────────────────────────────

    def plot_s11(self, output_path: Path | None = None) -> None:
        idx = int(np.argmin(self._s11_db))
        fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)

        ax1.plot(self._freq / 1e6, self._s11_db, "royalblue", lw=2, label="S11")
        ax1.axvline(915, color="crimson", ls="--", lw=1.5, label="915 MHz")
        ax1.axhline(-10, color="gray", ls=":", lw=1)
        ax1.axhline(-15, color="green", ls=":", lw=1, label="−15 dB")
        ax1.scatter(
            self._freq[idx] / 1e6,
            self._s11_db[idx],
            color="crimson",
            zorder=5,
            label=f"{self._freq[idx] / 1e6:.0f} MHz, {self._s11_db[idx]:.1f} dB",
        )
        ax1.set(
            xlabel="Frequency (MHz)",
            ylabel="S11 (dB)",
            title=(
                f"S11  N={self.N_MEANDERS}  V_gap={self.MEANDER_V_GAP_MM} mm"
                f"  feed_sep={self.FEED_SEPARATION_MM} mm"
            ),
            xlim=[self._freq[0] / 1e6, self._freq[-1] / 1e6],
            ylim=[-40, 5],
        )
        ax1.grid(True, alpha=0.35)
        ax1.legend(fontsize=8)

        ax2.plot(self._freq / 1e6, self._re_zin, "k-", lw=2, label="Re{Zin}")
        ax2.plot(self._freq / 1e6, self._im_zin, "r--", lw=2, label="Im{Zin}")
        ax2.axvline(915, color="royalblue", ls="--", lw=1.5)
        ax2.axhline(50, color="green", ls=":", lw=1.2, label="50 Ω")
        ax2.axhline(0, color="gray", ls="-", lw=0.8)
        ax2.axvline(
            self._f_res / 1e6,
            color="crimson",
            ls=":",
            lw=1.5,
            label=f"S11 min @ {self._f_res / 1e6:.0f} MHz  Re={self._re_zin[idx]:.0f} Ω",
        )
        ax2.set(
            xlabel="Frequency (MHz)",
            ylabel="Impedance (Ω)",
            xlim=[self._freq[0] / 1e6, self._freq[-1] / 1e6],
            ylim=[-200, 300],
        )
        ax2.grid(True, alpha=0.35)
        ax2.legend(fontsize=8)

        if output_path:
            plt.savefig(output_path, dpi=150, bbox_inches="tight")
        plt.show()

    def plot_far_field(
        self, db_floor: float = -20.0, output_path: Path | None = None
    ) -> None:
        theta = self._theta
        phi = self._phi
        dir_dbi = self._dir_dbi

        def clip(pattern: np.ndarray) -> np.ndarray:
            return np.clip(pattern, db_floor, None) - db_floor

        idx_phi_0 = int(np.argmin(np.abs(phi - 0.0)))
        idx_phi_90 = int(np.argmin(np.abs(phi - 90.0)))
        idx_phi_180 = int(np.argmin(np.abs(phi - 180.0)))
        idx_phi_270 = int(np.argmin(np.abs(phi - 270.0)))
        idx_theta_90 = int(np.argmin(np.abs(theta - 90.0)))

        # Full 360° elevation cuts: combine forward half and reversed back half
        def elevation_cut(fwd_phi_idx: int, back_phi_idx: int) -> tuple:
            fwd = dir_dbi[:, fwd_phi_idx]
            back = dir_dbi[::-1, back_phi_idx]
            angles = np.linspace(0, 2 * np.pi, len(fwd) + len(back), endpoint=False)
            return angles, clip(np.concatenate([fwd, back]))

        # Azimuth cut: close phi loop to avoid discontinuity at 0°/360°
        phi_closed = np.deg2rad(np.append(phi, phi[0] + 360.0))
        xy_closed = clip(np.append(dir_dbi[idx_theta_90, :], dir_dbi[idx_theta_90, 0]))

        fig, axes = plt.subplots(
            1, 3, subplot_kw={"projection": "polar"}, figsize=(15, 5)
        )
        fig.suptitle(
            f"Far-Field — {self._f_res / 1e6:.0f} MHz  Dmax = {self._dmax_dbi:.1f} dBi",
            fontsize=13,
        )

        plots = [
            (
                axes[0],
                *elevation_cut(idx_phi_0, idx_phi_180),
                "XZ elevation  (φ=0°/180°)",
            ),
            (
                axes[1],
                *elevation_cut(idx_phi_90, idx_phi_270),
                "YZ elevation  (φ=90°/270°)",
            ),
            (axes[2], phi_closed, xy_closed, "XY azimuth  (θ=90°)"),
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
        if output_path:
            plt.savefig(output_path, dpi=150, bbox_inches="tight")
        plt.show()

    # ── VTK export ─────────────────────────────────────────────────────

    def export_far_field_vtk(self, output_path: Path) -> None:
        dir_linear = np.maximum(10.0 ** (self._dir_dbi / 10.0), 0.0)
        r_grid = dir_linear * (self.FAR_FIELD_RADIUS_MM / dir_linear.max())

        # Close phi to eliminate seam
        phi_closed = np.append(self._phi, self._phi[0] + 360.0)
        r_closed = np.hstack([r_grid, r_grid[:, :1]])
        dbi_closed = np.hstack([self._dir_dbi, self._dir_dbi[:, :1]])

        n_theta, n_phi_c = len(self._theta), len(phi_closed)
        tg, pg = np.meshgrid(
            np.deg2rad(self._theta), np.deg2rad(phi_closed), indexing="ij"
        )
        x = (r_closed * np.sin(tg) * np.cos(pg)).ravel()
        y = (r_closed * np.sin(tg) * np.sin(pg)).ravel()
        z = (r_closed * np.cos(tg)).ravel()
        dbi_flat = dbi_closed.ravel()
        n_pts = n_theta * n_phi_c

        with open(output_path, "w") as f:
            f.write("# vtk DataFile Version 3.0\nIFA Far-Field Balloon\nASCII\n")
            f.write("DATASET STRUCTURED_GRID\n")
            f.write(f"DIMENSIONS {n_phi_c} {n_theta} 1\n")
            f.write(f"POINTS {n_pts} float\n")
            for xi, yi, zi in zip(x, y, z):
                f.write(f"{xi:.4f} {yi:.4f} {zi:.4f}\n")
            f.write(f"\nPOINT_DATA {n_pts}\n")
            f.write("SCALARS directivity_dBi float 1\nLOOKUP_TABLE default\n")
            for v in dbi_flat:
                f.write(f"{v:.4f}\n")
        print(f"  Far-field VTK → {output_path}")

    def export_pcb_vtk(self, output_path: Path) -> None:
        bt = self.BOARD_THICKNESS_MM
        bw, bl = self.BOARD_WIDTH_MM, self.BOARD_LENGTH_MM
        bx0, bx1 = -bw / 2, bw / 2
        by0, by1 = -bl / 2, bl / 2

        polys: list[list[tuple]] = []
        # Substrate face
        polys.append([(bx0, by0, 0), (bx1, by0, 0), (bx1, by1, 0), (bx0, by1, 0)])
        # GND plane
        gx = self.GND_X_MM
        polys.append([(gx, by0, bt), (bx1, by0, bt), (bx1, by1, bt), (gx, by1, bt)])
        # IFA traces
        for x0, y0, x1, y1 in self._trace_boxes:
            polys.append([(x0, y0, bt), (x1, y0, bt), (x1, y1, bt), (x0, y1, bt)])

        pts: list[tuple] = []
        pt_map: dict[tuple, int] = {}
        poly_ids: list[list[int]] = []

        def get_idx(p: tuple) -> int:
            key = (round(p[0], 4), round(p[1], 4), round(p[2], 4))
            if key not in pt_map:
                pt_map[key] = len(pts)
                pts.append(key)
            return pt_map[key]

        for poly in polys:
            poly_ids.append([get_idx(p) for p in poly])

        n_pts = len(pts)
        n_polys = len(poly_ids)
        cell_sz = sum(len(p) + 1 for p in poly_ids)

        with open(output_path, "w") as f:
            f.write(
                "# vtk DataFile Version 3.0\nPCB with IFA\nASCII\nDATASET POLYDATA\n"
            )
            f.write(f"POINTS {n_pts} float\n")
            for p in pts:
                f.write(f"{p[0]:.4f} {p[1]:.4f} {p[2]:.4f}\n")
            f.write(f"\nPOLYGONS {n_polys} {cell_sz}\n")
            for ids in poly_ids:
                f.write(f"{len(ids)} {' '.join(str(i) for i in ids)}\n")
            f.write(f"\nCELL_DATA {n_polys}\n")
            f.write("SCALARS region int 1\nLOOKUP_TABLE default\n")
            for i in range(n_polys):
                f.write(f"{0 if i == 0 else 1 if i == 1 else 2}\n")
        print(f"  PCB VTK       → {output_path}")


def main() -> None:
    sim = IFASimulation(Path(__file__).parent / "manual_sim")
    sim.run(preview=True, post_process_only=False)

    sim.compute_s_parameters()
    sim.plot_s11(sim.sim_dir / "s11_impedance.png")

    if sim._s11_db is not None and sim._s11_db.min() < -10.0:
        sim.compute_far_field()
        sim.plot_far_field(output_path=sim.sim_dir / "far_field_cuts.png")
        sim.export_far_field_vtk(sim.sim_dir / "far_field.vtk")
        sim.export_pcb_vtk(sim.sim_dir / "pcb_with_antenna.vtk")


if __name__ == "__main__":
    main()
