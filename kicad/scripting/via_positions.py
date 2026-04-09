import pcbnew

board = pcbnew.GetBoard()

# Get board bounding box center to use as (0,0) reference
bbox = board.GetBoardEdgesBoundingBox()
cx = bbox.GetCenter().x
cy = bbox.GetCenter().y

print(f"Board center (IU): {cx}, {cy}")
print(f"{'Ref':<6} {'X (mm)':>10} {'Y (mm)':>10} {'Size (mm)':>10}")
print("-" * 40)

for track in board.GetTracks():
    if track.GetClass() == "PCB_VIA":
        x_rel = pcbnew.ToMM(track.GetX() - cx)
        y_rel = pcbnew.ToMM(track.GetY() - cy)
        size  = pcbnew.ToMM(track.GetWidth())
        print(f"VIA    {x_rel:>10.4f} {y_rel:>10.4f} {size:>10.4f}")