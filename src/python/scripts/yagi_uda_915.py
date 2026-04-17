from __future__ import annotations

import subprocess
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from CSXCAD import ContinuousStructure
from openEMS import openEMS
from openEMS.physical_constants import C0, EPS0


class YagiSimulation:
    """openEMS FDTD simulation of a 915 MHz Yagi-Uda wire antenna.

    Axes: X = boom (+X = main beam), Y = elements, Z = height (elements at Z=0).
    Driven element at X=0; reflector at X<0; directors at X>0.
    Feed: 50 Ohm lumped port across FEED_GAP_MM at the driven element centre.

    Tuning:
      Resonance too high -> lengthen DRIVEN_HALF_LENGTH_MM
      Resonance too low  -> shorten  DRIVEN_HALF_LENGTH_MM
      Zin too high       -> decrease REFLECTOR_SPACING_MM
      Zin too low        -> increase REFLECTOR_SPACING_MM (typical Yagi: 25-35 Ohm)
      More gain          -> add directors or increase DIRECTOR_SPACINGS_MM
    """

    # ── Frequency ──────────────────────────────────────────────────────
    CENTER_FREQUENCY_HZ: float = 915e6
    # Wide BW = short Gaussian pulse = fewer timesteps. Do not go below 100 MHz.
    BANDWIDTH_HZ: float = 200e6

    # ── Wire ───────────────────────────────────────────────────────────
    # Solid copper core.  Change to match your actual build wire gauge.
    # AWG 12 -> 2.053 mm  |  AWG 14 -> 1.628 mm  |  AWG 16 -> 1.291 mm
    # AWG 18 -> 1.024 mm  |  AWG 20 -> 0.812 mm  |  AWG 22 -> 0.644 mm
    WIRE_DIAMETER_MM: float = 1.628  # AWG 18 solid copper  <- default

    # ── Driven element (split half-wave dipole) ─────────────────────────
    # DRIVEN_HALF_LENGTH_MM is the Y coordinate of the wire TIP in the sim,
    # measured from the dipole centre (Y = 0).  It is NOT the physical wire
    # length — the wire only spans from ±FEED_GAP_MM/2 to ±DRIVEN_HALF_LENGTH_MM.
    #
    # Physical wire length per arm = DRIVEN_HALF_LENGTH_MM - FEED_GAP_MM / 2
    #   With FEED_GAP = 8 mm:  69 - 4 = 65 mm  <- cut each arm to this length
    #
    # Resonant frequency is set by the TIP POSITION, not the wire length.
    # Changing FEED_GAP_MM does not require changing this value.
    # Resonance too high -> increase this; too low -> decrease this.
    # <- PRIMARY FREQUENCY TUNING KNOB
    DRIVEN_HALF_LENGTH_MM: float = 69.0

    # Physical air gap at the dipole centre where the coax feed is attached.
    # 8 mm chosen so that blind-hole depth = (BOOM_CROSS/2 - FEED_GAP/2)
    #   = (5 - 4) = 1 mm — the wire only enters the boom 1 mm and is secured
    #   with a dab of hot glue at the face.  8 mm of solid PETG fills the gap,
    #   guaranteeing no electrical contact.  At 915 MHz, 8 mm = 0.024*lambda,
    #   which has no measurable effect on radiation or impedance.
    # Must remain > WIRE_DIAMETER_MM (1.024 mm).  Do not reduce below 2 mm.
    FEED_GAP_MM: float = 8.0

    # ── Reflector ──────────────────────────────────────────────────────
    N_REFLECTORS: int = 1  # 0 = bare dipole only; 1 = standard Yagi

    # Total reflector element length.  Classical: 0.50 * lambda ~ 164 mm.
    # A longer reflector improves F/B but slightly shifts the resonance lower.
    REFLECTOR_LENGTH_MM: float = 167.0

    # Spacing from the driven element back to the reflector.
    # Classical: 0.20 * lambda ~ 66 mm.
    # Increasing spacing raises feed impedance and shifts the F/B peak.
    # <- IMPEDANCE / FRONT-TO-BACK TUNING KNOB
    REFLECTOR_SPACING_MM: float = 65.6

    # ── Directors ──────────────────────────────────────────────────────
    # 3 directors instead of 4: boom shrinks from 404 mm -> 324 mm.
    # Printed at 45 deg on a 256x256 mm plate the footprint is (324+12)/sqrt(2)
    # ~ 238 mm per side, which fits with ~18 mm to spare.
    # Expected gain: ~7.5-8.0 dBi (vs ~8.1 dBi with 4 directors — negligible loss).
    # All spacings >= 80 mm (>= 4 mesh cells at lambda/20) so FDTD mesh is adequate.
    N_DIRECTORS: int = 3

    # Total length of each director.  Exactly N_DIRECTORS entries required.
    # Directors are progressively shorter than the driven element.
    # Classical range: 0.43 - 0.45 * lambda -> 141 - 148 mm.
    # <- GAIN / BEAMWIDTH TUNING KNOB
    DIRECTOR_LENGTHS_MM: list[float] = [130.0, 127.0, 124.0]

    # Spacing from the PREVIOUS element (driven->dir1, dir1->dir2, etc.).
    # Exactly N_DIRECTORS entries required.  Classical: 0.25 - 0.35 * lambda.
    # All spacings kept >= 80 mm so there are always >= 4 mesh cells between
    # adjacent elements at lambda/20 resolution — avoids FDTD instability.
    # <- GAIN / IMPEDANCE TUNING KNOB
    DIRECTOR_SPACINGS_MM: list[float] = [98.4, 80.0, 80.0]

    # ── Optional dielectric boom ────────────────────────────────────────
    # Models the 3-D printed plastic boom running along the X axis.
    # PLA:  epsr ~ 2.7 - 3.0  |  PETG: epsr ~ 2.5  |  ABS: epsr ~ 2.5 - 3.5
    BOOM_ENABLED: bool = True  # enable for final validation; leave off for sweeps
    BOOM_CROSS_MM: float = 10.0  # square cross-section side length [mm]
    BOOM_EPSR: float = 2.6  # relative permittivity (PLA default)
    BOOM_LOSS_TANGENT: float = 0.01  # dielectric loss tangent at 915 MHz
    # Extra boom length extending past the reflector (rear) and last director (front).
    # Must be > WIRE_RADIUS_MM so no element is flush with the boom face in the sim.
    # Also controls the printed boom overhang in the OpenSCAD export.
    BOOM_OVERHANG_MM: float = 9.0

    # ── Simulation domain ───────────────────────────────────────────────
    # >= lambda/2 ~ 164 mm for NF2FF accuracy. Raise to lambda for high-fidelity run.
    SIM_PADDING_MM: float = 165.0

    # ── Solver ─────────────────────────────────────────────────────────
    MAX_TIMESTEPS: int = 100_000
    MAX_TIME_S: float = 150.0
    END_CRITERIA: float = 1e-4  # halt when stored energy < this * peak
    FIELD_DUMP_SUBSAMPLE: list[int] = [2, 2, 2]

    # ── Post-processing ─────────────────────────────────────────────────
    FAR_FIELD_RADIUS_MM: float = 500.0  # balloon radius for VTK export

    # ──────────────────────────────────────────────────────────────────────

    def __init__(self, sim_dir: Path) -> None:
        self.sim_dir = sim_dir
        sim_dir.mkdir(parents=True, exist_ok=True)

        # ── Derived scalars ────────────────────────────────────────────
        self.LAMBDA_MM: float = C0 / self.CENTER_FREQUENCY_HZ / 1e-3
        self.WIRE_RADIUS_MM: float = self.WIRE_DIAMETER_MM / 2.0
        # lambda/20 ~ 16.4 mm at 915 MHz; this is the global coarse cell size
        self.MESH_RES_MM: float = self.LAMBDA_MM / 20.0

        # ── Element layout (all coordinates in mm) ─────────────────────
        # Driven element fixed at X = 0; reflector at X < 0; directors X > 0
        self.X_DRIVEN: float = 0.0
        self.X_REFLECTOR: float = -self.REFLECTOR_SPACING_MM

        self.X_DIRECTORS: list[float] = []
        x = 0.0
        for spc in self.DIRECTOR_SPACINGS_MM:
            x += spc
            self.X_DIRECTORS.append(x)

        self.X_BOOM_START: float = (
            self.X_REFLECTOR if self.N_REFLECTORS else self.X_DRIVEN
        )
        self.X_BOOM_END: float = (
            self.X_DIRECTORS[-1] if self.X_DIRECTORS else self.X_DRIVEN
        )
        self.BOOM_LENGTH_MM: float = self.X_BOOM_END - self.X_BOOM_START

        # Widest half-element span across all elements (for bounding box)
        candidates: list[float] = [self.DRIVEN_HALF_LENGTH_MM]
        if self.N_REFLECTORS:
            candidates.append(self.REFLECTOR_LENGTH_MM / 2.0)
        candidates.extend(d / 2.0 for d in self.DIRECTOR_LENGTHS_MM)
        self.MAX_HALF_ELEMENT_MM: float = max(candidates)

        # openEMS objects — populated by setup() and build_geometry()
        self.FDTD: openEMS | None = None
        self.CSX: ContinuousStructure | None = None
        self.mesh = None
        self.port = None
        self.nf2ff = None

        # Results — populated by compute_*() methods
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
        self._print_summary()

    # ── Validation ─────────────────────────────────────────────────────

    def _validate(self) -> None:
        assert len(self.DIRECTOR_LENGTHS_MM) == self.N_DIRECTORS, (
            f"DIRECTOR_LENGTHS_MM has {len(self.DIRECTOR_LENGTHS_MM)} entries "
            f"but N_DIRECTORS = {self.N_DIRECTORS}"
        )
        assert len(self.DIRECTOR_SPACINGS_MM) == self.N_DIRECTORS, (
            f"DIRECTOR_SPACINGS_MM has {len(self.DIRECTOR_SPACINGS_MM)} entries "
            f"but N_DIRECTORS = {self.N_DIRECTORS}"
        )
        assert self.N_REFLECTORS in (0, 1), "Only 0 or 1 reflectors are supported"
        assert self.FEED_GAP_MM > self.WIRE_DIAMETER_MM, (
            f"FEED_GAP_MM ({self.FEED_GAP_MM} mm) must be > "
            f"WIRE_DIAMETER_MM ({self.WIRE_DIAMETER_MM} mm)"
        )
        if self.SIM_PADDING_MM < self.LAMBDA_MM / 2.0:
            print(
                f"  WARNING: SIM_PADDING_MM ({self.SIM_PADDING_MM:.0f} mm) < "
                f"lambda/2 = {self.LAMBDA_MM / 2.0:.0f} mm — NF2FF far-field "
                f"accuracy may be reduced.  S11 sweeps are still valid."
            )
        if self.N_DIRECTORS > 1:
            for i in range(self.N_DIRECTORS - 1):
                assert self.DIRECTOR_LENGTHS_MM[i] >= self.DIRECTOR_LENGTHS_MM[i + 1], (
                    f"Director lengths should be non-increasing: "
                    f"dir[{i}]={self.DIRECTOR_LENGTHS_MM[i]} mm < "
                    f"dir[{i + 1}]={self.DIRECTOR_LENGTHS_MM[i + 1]} mm"
                )

    def _print_summary(self) -> None:
        lam = self.LAMBDA_MM
        print("=" * 62)
        print("  915 MHz Yagi-Uda  —  openEMS FDTD Simulation")
        print(f"  lambda = {lam:.1f} mm    mesh Delta ~ {self.MESH_RES_MM:.1f} mm")
        print("=" * 62)
        if self.N_REFLECTORS:
            print(
                f"  Reflector   {self.REFLECTOR_LENGTH_MM:6.1f} mm"
                f"  ({self.REFLECTOR_LENGTH_MM / lam:.3f}lam)"
                f"  X = {self.X_REFLECTOR:.1f} mm"
            )
        driven_total = 2.0 * self.DRIVEN_HALF_LENGTH_MM
        print(
            f"  Driven      {driven_total:6.1f} mm"
            f"  ({driven_total / lam:.3f}lam)"
            f"  X = {self.X_DRIVEN:.1f} mm"
            f"  gap = {self.FEED_GAP_MM:.1f} mm"
        )
        for i, (xd, dl) in enumerate(zip(self.X_DIRECTORS, self.DIRECTOR_LENGTHS_MM)):
            print(
                f"  Director {i + 1}  {dl:6.1f} mm"
                f"  ({dl / lam:.3f}lam)"
                f"  X = {xd:.1f} mm"
            )
        print(
            f"  Boom length  {self.BOOM_LENGTH_MM:.1f} mm"
            f"  ({self.BOOM_LENGTH_MM / lam:.2f}lam)"
        )
        print(
            f"  Wire diam.   {self.WIRE_DIAMETER_MM:.3f} mm"
            f"  (r = {self.WIRE_RADIUS_MM:.3f} mm)"
        )
        if self.BOOM_ENABLED:
            print(
                f"  Dielectric boom  {self.BOOM_CROSS_MM:.0f}x{self.BOOM_CROSS_MM:.0f} mm"
                f"  epsr = {self.BOOM_EPSR}  tand = {self.BOOM_LOSS_TANGENT}"
            )
        print("=" * 62)

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
        self.mesh.SetDeltaUnit(1e-3)  # all coordinates in mm -> 1 unit = 1 mm

    # ── Geometry ───────────────────────────────────────────────────────

    def build_geometry(self) -> None:
        if self.BOOM_ENABLED:
            self._add_boom()
        self._add_reflector()
        self._add_driven_element()
        self._add_directors()
        self._add_lumped_port()
        self._add_efield_dump()
        self._add_nearfield_dump()
        self._finalize_mesh()
        self.nf2ff = self.FDTD.CreateNF2FFBox()

        nx, ny, nz = [len(self.mesh.GetLines(d)) for d in "xyz"]
        print(
            f"  Mesh  X={nx}  Y={ny}  Z={nz}"
            f"  cells ~ {(nx - 1) * (ny - 1) * (nz - 1):,}"
        )

    # ── Private geometry builders ──────────────────────────────────────

    def _add_wire_element(self, metal, x_pos: float, half_length: float) -> None:
        """Add a single full-length PEC cylinder at (x_pos, 0, 0) along Y."""
        metal.AddCylinder(
            start=[x_pos, -half_length, 0.0],
            stop=[x_pos, half_length, 0.0],
            radius=self.WIRE_RADIUS_MM,
            priority=10,
        )

    def _add_reflector(self) -> None:
        if self.N_REFLECTORS == 0:
            return
        ref = self.CSX.AddMetal("reflector")
        self._add_wire_element(ref, self.X_REFLECTOR, self.REFLECTOR_LENGTH_MM / 2.0)
        print(
            f"  + reflector   X={self.X_REFLECTOR:.1f} mm"
            f"  L={self.REFLECTOR_LENGTH_MM:.1f} mm"
        )

    def _add_driven_element(self) -> None:
        """
        Split dipole: two PEC half-cylinders with a feed gap at Y = 0.

        Lower arm spans  -DRIVEN_HALF_LENGTH_MM  to  -FEED_GAP_MM/2.
        Upper arm spans  +FEED_GAP_MM/2          to  +DRIVEN_HALF_LENGTH_MM.
        The lumped port will later fill the gap exactly.
        """
        drv = self.CSX.AddMetal("driven_element")
        r = self.WIRE_RADIUS_MM
        gap = self.FEED_GAP_MM / 2.0

        # Lower arm: element tip -> just short of the feed gap
        drv.AddCylinder(
            start=[self.X_DRIVEN, -self.DRIVEN_HALF_LENGTH_MM, 0.0],
            stop=[self.X_DRIVEN, -gap, 0.0],
            radius=r,
            priority=10,
        )
        # Upper arm: just past the feed gap -> element tip
        drv.AddCylinder(
            start=[self.X_DRIVEN, gap, 0.0],
            stop=[self.X_DRIVEN, self.DRIVEN_HALF_LENGTH_MM, 0.0],
            radius=r,
            priority=10,
        )
        print(
            f"  + driven      X={self.X_DRIVEN:.1f} mm"
            f"  L={2.0 * self.DRIVEN_HALF_LENGTH_MM:.1f} mm"
            f"  gap={self.FEED_GAP_MM:.1f} mm"
        )

    def _add_directors(self) -> None:
        for i, (x_pos, length) in enumerate(
            zip(self.X_DIRECTORS, self.DIRECTOR_LENGTHS_MM)
        ):
            metal = self.CSX.AddMetal(f"director_{i + 1}")
            self._add_wire_element(metal, x_pos, length / 2.0)
            print(f"  + director {i + 1}  X={x_pos:.1f} mm  L={length:.1f} mm")

    def _add_lumped_port(self) -> None:
        r = self.WIRE_RADIUS_MM
        gap = self.FEED_GAP_MM / 2.0
        self.port = self.FDTD.AddLumpedPort(
            port_nr=1,
            R=50,
            start=[self.X_DRIVEN - r, -gap, -r],
            stop=[self.X_DRIVEN + r, gap, r],
            p_dir="y",
            excite=True,
            priority=5,
        )
        print(f"  + lumped port  50 Ohm  Y-dir  gap={self.FEED_GAP_MM:.1f} mm")

    def _add_boom(self) -> None:
        kappa = (
            self.BOOM_LOSS_TANGENT
            * 2.0
            * np.pi
            * self.CENTER_FREQUENCY_HZ
            * EPS0
            * self.BOOM_EPSR
        )
        boom_mat = self.CSX.AddMaterial(
            "boom",
            epsilon=self.BOOM_EPSR,
            kappa=kappa,
        )
        hc = self.BOOM_CROSS_MM / 2.0
        boom_mat.AddBox(
            start=[self.X_BOOM_START - self.BOOM_OVERHANG_MM, -hc, -hc],
            stop=[self.X_BOOM_END + self.BOOM_OVERHANG_MM, hc, hc],
            priority=1,
        )
        print(
            f"  + dielectric boom  {self.BOOM_CROSS_MM}x{self.BOOM_CROSS_MM} mm"
            f"  epsr={self.BOOM_EPSR}  tand={self.BOOM_LOSS_TANGENT}"
        )

    def _add_efield_dump(self) -> None:
        """Subsampled E-field time-domain dump around the antenna for ParaView."""
        pad = 30.0
        dump = self.CSX.AddDump(
            "Et", dump_type=0, file_type=0, sub_sampling=self.FIELD_DUMP_SUBSAMPLE
        )
        dump.AddBox(
            start=[
                self.X_BOOM_START - pad,
                -self.MAX_HALF_ELEMENT_MM - pad,
                -pad / 2.0,
            ],
            stop=[
                self.X_BOOM_END + pad,
                self.MAX_HALF_ELEMENT_MM + pad,
                pad / 2.0,
            ],
        )

    def _add_nearfield_dump(self) -> None:
        """H-field snapshot at Z = 0 to inspect induced current distribution."""
        pad = 30.0
        dump = self.CSX.AddDump("Hplane", dump_type=1, file_type=0)
        dump.AddBox(
            start=[
                self.X_BOOM_START - pad,
                -self.MAX_HALF_ELEMENT_MM - pad,
                0.0,
            ],
            stop=[
                self.X_BOOM_END + pad,
                self.MAX_HALF_ELEMENT_MM + pad,
                0.0,
            ],
        )

    def _finalize_mesh(self) -> None:
        """Graded mesh: ±wire_radius lines at each element in X and Z resolve the
        wire cross-section; global SmoothMeshLines expands to MESH_RES_MM elsewhere.
        Do NOT call SmoothMeshLines on elem_x — that fills inter-element gaps at
        sub-mm steps and was the cause of the original 473-line / 76-min blow-up."""
        r = self.WIRE_RADIUS_MM
        gap = self.FEED_GAP_MM / 2.0
        pad = self.SIM_PADDING_MM

        # ── Simulation bounding box ────────────────────────────────────
        x_min = self.X_BOOM_START - pad
        x_max = self.X_BOOM_END + pad
        y_abs = self.MAX_HALF_ELEMENT_MM + pad
        z_abs = pad / 2.0

        self.mesh.AddLine("x", [x_min, x_max])
        self.mesh.AddLine("y", [-y_abs, y_abs])
        self.mesh.AddLine("z", [-z_abs, z_abs])

        # X: three lines per element (x-r, x, x+r) — global smooth fills the gaps
        elem_x: list[float] = []
        if self.N_REFLECTORS:
            elem_x += [self.X_REFLECTOR - r, self.X_REFLECTOR, self.X_REFLECTOR + r]
        elem_x += [self.X_DRIVEN - r, self.X_DRIVEN, self.X_DRIVEN + r]
        for x_pos in self.X_DIRECTORS:
            elem_x += [x_pos - r, x_pos, x_pos + r]
        self.mesh.AddLine("x", elem_x)

        # Y: element tips + feed gap edges
        key_y: list[float] = [0.0, -gap, gap]
        if self.N_REFLECTORS:
            key_y += [
                -self.REFLECTOR_LENGTH_MM / 2.0,
                self.REFLECTOR_LENGTH_MM / 2.0,
            ]
        key_y += [-self.DRIVEN_HALF_LENGTH_MM, self.DRIVEN_HALF_LENGTH_MM]
        for length in self.DIRECTOR_LENGTHS_MM:
            key_y += [-length / 2.0, length / 2.0]
        self.mesh.AddLine("y", sorted(set(key_y)))

        # Z: ±wire_radius at element plane; sets CFL timestep (~1 ps)
        self.mesh.AddLine("z", [-r, 0.0, r])

        if self.BOOM_ENABLED:
            hc = self.BOOM_CROSS_MM / 2.0
            self.mesh.AddLine("y", [-hc, hc])
            self.mesh.AddLine("z", [-hc, hc])

        self.mesh.SmoothMeshLines("all", self.MESH_RES_MM, 1.4)

    # ── Run ────────────────────────────────────────────────────────────

    def preview(self) -> None:
        """Write geometry XML and launch AppCSXCAD for a visual sanity check."""
        xml_path = self.sim_dir / "yagi_915.xml"
        self.CSX.Write2XML(str(xml_path))
        try:
            subprocess.Popen(["AppCSXCAD", str(xml_path)]).wait()
        except FileNotFoundError:
            print("  AppCSXCAD not found — XML written, skipping preview")

    def run(self, preview: bool = True, post_process_only: bool = False) -> None:
        self.setup()
        self.build_geometry()
        if preview:
            self.preview()
        input("\nPress [ENTER] to start FDTD simulation, Ctrl+C to abort.\n")
        if not post_process_only:
            self.FDTD.Run(str(self.sim_dir), cleanup=True)

    # ── Post-processing ────────────────────────────────────────────────

    def compute_s_parameters(self, n_points: int = 501) -> None:
        f_start = max(self.CENTER_FREQUENCY_HZ - 2.0 * self.BANDWIDTH_HZ, 800e6)
        f_stop = self.CENTER_FREQUENCY_HZ + 2.0 * self.BANDWIDTH_HZ
        self._freq = np.linspace(f_start, f_stop, n_points)

        self.port.CalcPort(str(self.sim_dir), self._freq)
        s11 = self.port.uf_ref / self.port.uf_inc
        self._s11_db = 20.0 * np.log10(np.abs(s11) + 1e-30)

        zin = self.port.uf_tot / self.port.if_tot
        self._re_zin = np.real(zin)
        self._im_zin = np.imag(zin)

        idx = int(np.argmin(self._s11_db))
        self._f_res = float(self._freq[idx])
        print(
            f"  S11 min @ {self._f_res / 1e6:.1f} MHz  ->  {self._s11_db[idx]:.1f} dB"
            f"  Zin = {self._re_zin[idx]:.1f} + j{self._im_zin[idx]:.1f} Ohm"
        )

    def compute_far_field(self) -> None:
        self._theta = np.arange(0.0, 181.0, 2.0)
        self._phi = np.arange(0.0, 360.0, 5.0)
        result = self.nf2ff.CalcNF2FF(
            str(self.sim_dir),
            self._f_res,
            self._theta,
            self._phi,
            center=[0.0, 0.0, 0.0],
            read_cached=True,
            outfile="nf2ff_result.h5",
        )
        e_norm = result.E_norm[0]
        dmax = result.Dmax[0]
        self._dir_dbi = 10.0 * np.log10(dmax * (e_norm / np.max(e_norm)) ** 2 + 1e-30)
        self._dmax_dbi = 10.0 * np.log10(dmax)
        print(f"  Dmax = {self._dmax_dbi:.1f} dBi")

    # ── Plots ──────────────────────────────────────────────────────────

    def plot_s11(self, output_path: Path | None = None) -> None:
        if self._s11_db is None or self._freq is None:
            raise RuntimeError("Run compute_s_parameters() first.")

        idx = int(np.argmin(self._s11_db))
        fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 9), tight_layout=True)

        # ── S11 ──────────────────────────────────────────────────────
        ax1.plot(self._freq / 1e6, self._s11_db, "royalblue", lw=2, label="S11")
        ax1.axvline(915, color="crimson", ls="--", lw=1.5, label="915 MHz")
        ax1.axhline(-10, color="gray", ls=":", lw=1.0, label="-10 dB")
        ax1.axhline(-15, color="limegreen", ls=":", lw=1.0, label="-15 dB")
        ax1.scatter(
            self._freq[idx] / 1e6,
            self._s11_db[idx],
            color="crimson",
            zorder=5,
            s=60,
            label=f"{self._freq[idx] / 1e6:.0f} MHz  {self._s11_db[idx]:.1f} dB",
        )
        ax1.set(
            xlabel="Frequency (MHz)",
            ylabel="S11 (dB)",
            title=(
                f"Yagi-Uda 915 MHz  —  "
                f"{self.N_REFLECTORS}R + D + {self.N_DIRECTORS}D  "
                f"wire diam {self.WIRE_DIAMETER_MM:.2f} mm"
            ),
            xlim=[self._freq[0] / 1e6, self._freq[-1] / 1e6],
            ylim=[-40, 5],
        )
        ax1.grid(True, alpha=0.35)
        ax1.legend(fontsize=8)

        # ── Input impedance ───────────────────────────────────────────
        ax2.plot(self._freq / 1e6, self._re_zin, "k-", lw=2, label="Re{Zin}")
        ax2.plot(self._freq / 1e6, self._im_zin, "r--", lw=2, label="Im{Zin}")
        ax2.axvline(915, color="royalblue", ls="--", lw=1.5)
        ax2.axhline(50, color="limegreen", ls=":", lw=1.2, label="50 Ohm target")
        ax2.axhline(0, color="gray", ls="-", lw=0.8)
        ax2.axvline(
            self._f_res / 1e6,
            color="crimson",
            ls=":",
            lw=1.5,
            label=(
                f"S11 min @ {self._f_res / 1e6:.0f} MHz  "
                f"Re = {self._re_zin[idx]:.0f} Ohm"
            ),
        )
        ax2.set(
            xlabel="Frequency (MHz)",
            ylabel="Impedance (Ohm)",
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
        if self._dir_dbi is None:
            raise RuntimeError("Run compute_far_field() first.")

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

        # Full 360-degree elevation cuts: forward half + reversed back half
        def elevation_cut(fwd_phi_idx: int, bk_phi_idx: int) -> tuple:
            fwd = dir_dbi[:, fwd_phi_idx]
            bk = dir_dbi[::-1, bk_phi_idx]
            angles = np.linspace(0, 2 * np.pi, len(fwd) + len(bk), endpoint=False)
            return angles, clip(np.concatenate([fwd, bk]))

        # Azimuth cut — close phi loop to prevent discontinuity at 0/360
        phi_closed = np.deg2rad(np.append(phi, phi[0] + 360.0))
        az_closed = clip(np.append(dir_dbi[idx_theta_90, :], dir_dbi[idx_theta_90, 0]))

        fig, axes = plt.subplots(
            1, 3, subplot_kw={"projection": "polar"}, figsize=(15, 5)
        )
        fig.suptitle(
            f"Far-Field  —  {self._f_res / 1e6:.0f} MHz"
            f"   Dmax = {self._dmax_dbi:.1f} dBi\n"
            f"Yagi {self.N_REFLECTORS}R + D + {self.N_DIRECTORS}D  "
            f"(boom = +X, elements along Y, main beam at phi=0 theta=90)",
            fontsize=11,
        )

        # For a Y-polarised Yagi with boom along +X:
        #   H-plane: XZ plane (phi=0/180 elevation) — perpendicular to elements
        #   E-plane: YZ plane (phi=90/270 elevation) — contains E-field vector
        #   Azimuth: XY plane (theta=90) — top-down view showing F/B and side lobes
        plots = [
            (
                axes[0],
                *elevation_cut(idx_phi_0, idx_phi_180),
                "H-plane (XZ)  phi=0/180",
            ),
            (
                axes[1],
                *elevation_cut(idx_phi_90, idx_phi_270),
                "E-plane (YZ)  phi=90/270",
            ),
            (axes[2], phi_closed, az_closed, "Azimuth (XY)  theta=90"),
        ]
        for ax, angles, pattern, title in plots:
            ax.plot(angles, pattern, "royalblue", lw=2)
            ax.set_title(title, pad=12, fontsize=9)
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
        """Export the 3-D far-field balloon as a VTK StructuredGrid for ParaView."""
        if self._dir_dbi is None:
            raise RuntimeError("Run compute_far_field() first.")

        dir_linear = np.maximum(10.0 ** (self._dir_dbi / 10.0), 0.0)
        r_grid = dir_linear * (self.FAR_FIELD_RADIUS_MM / dir_linear.max())

        # Close phi to seal the seam at 0/360 degrees
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
            f.write("# vtk DataFile Version 3.0\nYagi Far-Field Balloon\nASCII\n")
            f.write("DATASET STRUCTURED_GRID\n")
            f.write(f"DIMENSIONS {n_phi_c} {n_theta} 1\n")
            f.write(f"POINTS {n_pts} float\n")
            for xi, yi, zi in zip(x, y, z):
                f.write(f"{xi:.4f} {yi:.4f} {zi:.4f}\n")
            f.write(f"\nPOINT_DATA {n_pts}\n")
            f.write("SCALARS directivity_dBi float 1\nLOOKUP_TABLE default\n")
            for v in dbi_flat:
                f.write(f"{v:.4f}\n")
        print(f"  Far-field VTK  ->  {output_path}")

    # ── OpenSCAD boom generator ─────────────────────────────────────────

    def export_openscad_boom(self, output_path: Path) -> None:
        """
        Write a parametric OpenSCAD file for the 3-D printed boom.

        The generated file produces a rectangular boom with perpendicular
        through-holes for each wire element.  Wire holes are oversized by
        PRINT_TOL_MM per side to account for FDM dimensional tolerances
        (typically 0.1 - 0.2 mm per side for well-calibrated printers).

        Build instructions:
          - Open in OpenSCAD and press F5 to preview, F6 to render.
          - Export as STL with File -> Export -> Export as STL.
          - Slice with your preferred slicer (e.g. PrusaSlicer, Cura).
          - Print at 30-40% infill in PLA or PETG.
          - Thread wire elements through the holes; friction-fit or secure
            with a small dab of CA glue or hot glue at each element end.

        Note: the driven element hole should be split into two separate
        offset holes (or a wider slot) to accommodate the feed gap between
        the two arms.  Edit the generated file and replace the driven-element
        cylinder with two cylinders offset by +/- FEED_GAP_MM/2 in X.
        """
        lines: list[str] = []
        a = lines.append

        a(
            "// 915 MHz Yagi-Uda boom — auto-generated by YagiSimulation.export_openscad_boom()"
        )
        a(
            "// Verify WIRE_DIA and PRINT_TOL_MM match your wire and printer before printing."
        )
        a("// All dimensions in mm.")
        a("")
        a(
            f"WIRE_DIA      = {self.WIRE_DIAMETER_MM:.4f};  // match your actual wire gauge"
        )
        a(
            "PRINT_TOL_MM  = 0.15;                         // extra hole radius per side for FDM tolerance"
        )
        a(
            f"BOOM_CROSS    = {self.BOOM_CROSS_MM:.1f};     // square cross-section side length"
        )
        a(
            f"BOOM_OVERHANG = {self.BOOM_OVERHANG_MM:.1f};                         // extend boom this far past end elements"
        )
        a(
            f"FEED_GAP      = {self.FEED_GAP_MM:.2f};       // gap between the two driven-element holes"
        )
        a("")
        a("// ── Element X positions (relative to driven element at X = 0) ──")
        if self.N_REFLECTORS:
            a(f"X_REFL  = {self.X_REFLECTOR:.2f};  // reflector")
        a(f"X_DRV   = {self.X_DRIVEN:.2f};   // driven element")
        for i, xd in enumerate(self.X_DIRECTORS):
            a(f"X_DIR{i + 1}  = {xd:.2f};   // director {i + 1}")
        a("")

        # Build element position list for the for-loop (exclude driven element,
        # which needs special split-hole treatment)
        passive_names: list[str] = []
        if self.N_REFLECTORS:
            passive_names.append("X_REFL")
        for i in range(self.N_DIRECTORS):
            passive_names.append(f"X_DIR{i + 1}")

        a(
            f"PASSIVE_XS = [{', '.join(passive_names)}];  // reflector + directors (continuous holes)"
        )
        a("")
        a(f"X_BOOM_START = {self.X_BOOM_START:.2f} - BOOM_OVERHANG;")
        a(f"X_BOOM_END   = {self.X_BOOM_END:.2f}   + BOOM_OVERHANG;")
        a("BOOM_LEN     = X_BOOM_END - X_BOOM_START;")
        a("")
        a("HOLE_R = (WIRE_DIA / 2.0) + PRINT_TOL_MM;")
        a("")
        a("$fn = 40;")
        a("")
        a("module yagi_boom() {")
        a("    difference() {")
        a("")
        a("        // ── Main boom body ──────────────────────────────────────")
        a("        translate([X_BOOM_START, -BOOM_CROSS / 2, -BOOM_CROSS / 2])")
        a("            cube([BOOM_LEN, BOOM_CROSS, BOOM_CROSS]);")
        a("")
        a("        // ── Passive element holes (reflector + directors) ────────")
        a("        // Full through-holes along Y, centred at Z = 0")
        a("        for (xp = PASSIVE_XS) {")
        a("            translate([xp, 0, 0])")
        a("                rotate([90, 0, 0])")
        a("                    cylinder(")
        a("                        h = BOOM_CROSS + 2,")
        a("                        r = HOLE_R,")
        a("                        center = true")
        a("                    );")
        a("        }")
        a("")
        a("        // ── Driven element: two blind holes at Z=0 from opposite faces ──")
        a(
            "        // Both holes sit at Z = 0, matching the simulation geometry exactly."
        )
        a("        // Arm A enters from the -Y face; Arm B from the +Y face.")
        a(
            "        // Each hole is (BOOM_CROSS/2 - FEED_GAP/2) deep, so each wire bottoms"
        )
        a("        // out exactly at the feed-gap edge.  The FEED_GAP of solid printed")
        a(
            "        // plastic between the two blind ends guarantees no contact / no short."
        )
        a("        //")
        a("        // Wire length per arm  =  DRIVEN_HALF_LENGTH - FEED_GAP/2")
        a(
            f"        //                      ="
            f"  {self.DRIVEN_HALF_LENGTH_MM:.0f} - {self.FEED_GAP_MM / 2:.0f}"
            f"  =  {self.DRIVEN_HALF_LENGTH_MM - self.FEED_GAP_MM / 2:.0f} mm"
        )
        a(
            "        // Push arm from its face until it bottoms out, then CA-glue at face."
        )
        a(
            "        // Coax: centre conductor soldered at -Y face arm, braid at +Y face arm."
        )
        a(
            f"        BLIND_DEPTH = BOOM_CROSS / 2 - FEED_GAP / 2;"
            f"  // = {self.BOOM_CROSS_MM / 2 - self.FEED_GAP_MM / 2:.1f} mm"
            f" ({self.BOOM_CROSS_MM:.0f} mm boom, {self.FEED_GAP_MM:.0f} mm gap)"
        )
        a("")
        a(
            "        // Arm A: enters -Y face, blind hole going +Y, stops at Y = -FEED_GAP/2"
        )
        a("        translate([X_DRV, -(BOOM_CROSS / 2) - 1, 0])")
        a("            rotate([-90, 0, 0])")
        a("                cylinder(h = BLIND_DEPTH + 1, r = HOLE_R);")
        a("")
        a(
            "        // Arm B: enters +Y face, blind hole going -Y, stops at Y = +FEED_GAP/2"
        )
        a("        translate([X_DRV, (BOOM_CROSS / 2) + 1, 0])")
        a("            rotate([90, 0, 0])")
        a("                cylinder(h = BLIND_DEPTH + 1, r = HOLE_R);")
        a("    }")
        a("}")
        a("")
        a("yagi_boom();")

        output_path.write_text("\n".join(lines) + "\n")
        print(f"  OpenSCAD boom  ->  {output_path}")


def main() -> None:
    sim = YagiSimulation(Path(__file__).parent / "yagi_sim")
    sim.run(preview=True, post_process_only=False)

    sim.compute_s_parameters()
    sim.plot_s11(sim.sim_dir / "yagi_s11.png")

    if sim._s11_db is not None and sim._s11_db.min() < -10.0:
        sim.compute_far_field()
        sim.plot_far_field(output_path=sim.sim_dir / "yagi_far_field.png")
        sim.export_far_field_vtk(sim.sim_dir / "yagi_far_field.vtk")

    sim.export_openscad_boom(sim.sim_dir / "yagi_boom.scad")


if __name__ == "__main__":
    main()
