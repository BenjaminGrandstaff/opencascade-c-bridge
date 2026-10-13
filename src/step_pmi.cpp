/*
 * Semantic AP242 GD&T on STEP assembly parts: datums, geometric tolerances
 * and size dimensions attached to part faces through OCCT's XCAF dimension
 * and tolerance tool, and verified by reading the written file back.
 */

#include "step_pmi.hpp"

#include "bridge_internal.hpp"

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Interface_InterfaceModel.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <StepAP214_AppliedApprovalAssignment.hxx>
#include <StepAP214_ApprovalItem.hxx>
#include <StepAP214_HArray1OfApprovalItem.hxx>
#include <StepAP242_DraughtingModelItemAssociation.hxx>
#include <StepBasic_Approval.hxx>
#include <StepBasic_ApprovalDateTime.hxx>
#include <StepBasic_ApprovalPersonOrganization.hxx>
#include <StepBasic_ApprovalRole.hxx>
#include <StepBasic_ApprovalStatus.hxx>
#include <StepBasic_CalendarDate.hxx>
#include <StepBasic_DateTimeSelect.hxx>
#include <StepBasic_Organization.hxx>
#include <StepBasic_Person.hxx>
#include <StepBasic_PersonAndOrganization.hxx>
#include <StepBasic_PersonOrganizationSelect.hxx>
#include <StepBasic_Product.hxx>
#include <StepBasic_ProductDefinition.hxx>
#include <StepBasic_ProductDefinitionFormation.hxx>
#include <StepRepr_NextAssemblyUsageOccurrence.hxx>
#include <StepAP242_GeometricItemSpecificUsage.hxx>
#include <StepBasic_ConversionBasedUnitAndLengthUnit.hxx>
#include <StepBasic_HArray1OfNamedUnit.hxx>
#include <StepBasic_MeasureWithUnit.hxx>
#include <StepBasic_SiUnitAndLengthUnit.hxx>
#include <StepData_StepModel.hxx>
#include <StepDimTol_Datum.hxx>
#include <StepBasic_LengthMeasureWithUnit.hxx>
#include <StepBasic_MeasureValueMember.hxx>
#include <StepBasic_PlaneAngleMeasureWithUnit.hxx>
#include <StepDimTol_GeometricTolerance.hxx>
#include <StepDimTol_GeometricToleranceTarget.hxx>
#include <StepDimTol_ProjectedZoneDefinition.hxx>
#include <StepDimTol_RunoutZoneDefinition.hxx>
#include <StepDimTol_RunoutZoneOrientation.hxx>
#include <StepDimTol_ToleranceZone.hxx>
#include <StepRepr_HArray1OfShapeAspect.hxx>
#include <StepGeom_Axis2Placement.hxx>
#include <StepGeom_Axis2Placement3d.hxx>
#include <StepGeom_CartesianPoint.hxx>
#include <StepGeom_Direction.hxx>
#include <StepGeom_GeomRepContextAndGlobUnitAssCtxAndGlobUncertaintyAssCtx.hxx>
#include <StepRepr_GlobalUnitAssignedContext.hxx>
#include <StepRepr_HArray1OfRepresentationItem.hxx>
#include <StepRepr_ShapeAspect.hxx>
#include <StepRepr_ShapeAspectRelationship.hxx>
#include <StepShape_ShapeRepresentation.hxx>
#include <StepGeom_Plane.hxx>
#include <StepVisual_CameraModelD3.hxx>
#include <StepVisual_CameraModelD3MultiClipping.hxx>
#include <StepVisual_CameraModelD3MultiClippingInterectionSelect.hxx>
#include <StepVisual_HArray1OfCameraModelD3MultiClippingInterectionSelect.hxx>
#include <XCAFDoc_ClippingPlaneTool.hxx>
#include <gp_Pln.hxx>
#include <StepVisual_CoordinatesList.hxx>
#include <StepVisual_DraughtingCallout.hxx>
#include <StepVisual_DraughtingModel.hxx>
#include <StepVisual_PlanarBox.hxx>
#include <StepVisual_TessellatedAnnotationOccurrence.hxx>
#include <StepVisual_TessellatedCurveSet.hxx>
#include <StepVisual_TessellatedGeometricSet.hxx>
#include <StepVisual_ViewVolume.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_HArray1OfXYZ.hxx>
#include <TopoDS_Compound.hxx>
#include <XCAFDoc_View.hxx>
#include <XCAFDoc_ViewTool.hxx>
#include <XCAFView_Object.hxx>
#include <TransferBRep.hxx>
#include <Transfer_TransientProcess.hxx>
#include <XSControl_TransferReader.hxx>
#include <XSControl_WorkSession.hxx>
#include <TCollection_HAsciiString.hxx>
#include <TDF_LabelSequence.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDimTolObjects_DatumObject.hxx>
#include <XCAFDimTolObjects_DimensionObject.hxx>
#include <XCAFDimTolObjects_GeomToleranceObject.hxx>
#include <XCAFDoc_Datum.hxx>
#include <XCAFDoc_DimTolTool.hxx>
#include <XCAFDoc_Dimension.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_GeomTolerance.hxx>
#include <XCAFDoc_ShapeTool.hxx>

#include <algorithm>
#include <array>
#include <cstdlib>
#include <cstring>
#include <optional>
#include <cmath>
#include <map>
#include <set>
#include <string>
#include <tuple>
#include <unordered_map>
#include <utility>

namespace occt_bridge_internal {

namespace {

constexpr uint32_t kModifierBits =
    OCCT_BRIDGE_GDT_FREE_STATE | OCCT_BRIDGE_GDT_STATISTICAL | OCCT_BRIDGE_GDT_TANGENT_PLANE;

const XCAFDimTolObjects_GeomToleranceType kTypes[] = {
    XCAFDimTolObjects_GeomToleranceType_Straightness,
    XCAFDimTolObjects_GeomToleranceType_Flatness,
    XCAFDimTolObjects_GeomToleranceType_CircularityOrRoundness,
    XCAFDimTolObjects_GeomToleranceType_Cylindricity,
    XCAFDimTolObjects_GeomToleranceType_ProfileOfLine,
    XCAFDimTolObjects_GeomToleranceType_ProfileOfSurface,
    XCAFDimTolObjects_GeomToleranceType_Parallelism,
    XCAFDimTolObjects_GeomToleranceType_Perpendicularity,
    XCAFDimTolObjects_GeomToleranceType_Angularity,
    XCAFDimTolObjects_GeomToleranceType_Position,
    XCAFDimTolObjects_GeomToleranceType_CircularRunout,
    XCAFDimTolObjects_GeomToleranceType_TotalRunout,
};
constexpr int32_t kTypeCount = static_cast<int32_t>(sizeof(kTypes) / sizeof(kTypes[0]));

// Part-local faces of each component, indexed on first use.
class ComponentFaces {
public:
    explicit ComponentFaces(const std::vector<const TopoDS_Shape*>& shapes) : shapes_(shapes) {}

    const TopTools_IndexedMapOfShape& of(size_t component) {
        auto found = faces_.find(component);
        if (found == faces_.end()) {
            const TopoDS_Shape local = shapes_[component]->Located(TopLoc_Location());
            found = faces_.emplace(component, ordered(local, TopAbs_FACE)).first;
        }
        return found->second;
    }

    const TopoDS_Face& face(size_t component, size_t index) {
        return TopoDS::Face(of(component)(static_cast<int>(index) + 1));
    }

private:
    const std::vector<const TopoDS_Shape*>& shapes_;
    std::map<size_t, TopTools_IndexedMapOfShape> faces_;
};

bool same_part(const TopoDS_Shape& first, const TopoDS_Shape& second) {
    return first.TShape() == second.TShape() && first.Orientation() == second.Orientation();
}

const char* faces_error(const occt_bridge_step_pmi_faces_t& target, ComponentFaces& faces, size_t components) {
    if (target.component >= components) {
        return "a PMI item names an unknown component";
    }
    if (target.faces == nullptr || target.face_count == 0) {
        return "every PMI item needs faces";
    }
    const size_t count = static_cast<size_t>(faces.of(target.component).Extent());
    for (size_t index = 0; index < target.face_count; ++index) {
        if (target.faces[index] >= count) {
            return "a PMI item names a face its component does not have";
        }
    }
    return nullptr;
}

bool positive(double value) {
    return std::isfinite(value) && value > 0.0;
}

const char* tolerance_error(
    const occt_bridge_step_tolerance_t& tolerance,
    const occt_bridge_step_pmi_t& pmi,
    const std::vector<const TopoDS_Shape*>& shapes) {
    if (tolerance.characteristic < 0 || tolerance.characteristic >= kTypeCount) {
        return "unknown geometric characteristic";
    }
    if (!positive(tolerance.value_mm)) {
        return "a tolerance value must be finite and positive";
    }
    if (tolerance.diameter != 0 && tolerance.diameter != 1) {
        return "diameter must be 0 or 1";
    }
    if (tolerance.material < OCCT_BRIDGE_MATERIAL_RFS || tolerance.material > OCCT_BRIDGE_MATERIAL_LMC) {
        return "unknown material condition";
    }
    if ((tolerance.modifiers & ~kModifierBits) != 0) {
        return "unknown tolerance modifier";
    }
    if (!std::isfinite(tolerance.projected_mm) || tolerance.projected_mm < 0.0) {
        return "a projected zone height must be finite and not negative";
    }
    if (tolerance.datum_count != 0 && tolerance.datums == nullptr) {
        return "tolerance datum references are missing";
    }
    uint32_t used = 0;
    for (size_t index = 0; index < tolerance.datum_count; ++index) {
        const auto& reference = tolerance.datums[index];
        if (reference.datum >= pmi.datum_count) {
            return "a tolerance references an unknown datum";
        }
        if (reference.compartment > 2) {
            return "datum compartments are 0, 1 and 2";
        }
        if (reference.boundary < OCCT_BRIDGE_DATUM_RFS || reference.boundary > OCCT_BRIDGE_DATUM_LMB) {
            return "unknown datum material boundary";
        }
        if (reference.translation != 0 && reference.translation != 1) {
            return "translation must be 0 or 1";
        }
        const auto& datum = pmi.datums[reference.datum];
        if (!same_part(*shapes[datum.target.component], *shapes[tolerance.target.component])) {
            return "a tolerance's datums must lie on its own part";
        }
        used |= 1u << reference.compartment;
    }
    // Compartments fill from the primary: A, A|B, A|B|C.
    if (used != 0 && (used & (used + 1)) != 0) {
        return "datum compartments must fill from the primary";
    }
    return nullptr;
}

const char* size_error(const occt_bridge_step_size_t& size, ComponentFaces& faces) {
    if (!positive(size.lower_mm) || !positive(size.upper_mm) || size.lower_mm > size.upper_mm) {
        return "size limits must be finite and positive, lower not above upper";
    }
    const auto& target = size.target;
    if (size.kind == OCCT_BRIDGE_STEP_SIZE_DIAMETER) {
        double radius = -1.0;
        for (size_t index = 0; index < target.face_count; ++index) {
            const BRepAdaptor_Surface surface(faces.face(target.component, target.faces[index]));
            if (surface.GetType() != GeomAbs_Cylinder) {
                return "a diameter must lie on cylindrical faces";
            }
            const double r = surface.Cylinder().Radius();
            if (radius >= 0.0 && std::abs(r - radius) > 1e-7 * std::max(1.0, radius)) {
                return "a diameter's cylindrical faces must share one radius";
            }
            radius = r;
        }
        return nullptr;
    }
    if (size.kind == OCCT_BRIDGE_STEP_SIZE_WIDTH) {
        if (target.face_count != 2) {
            return "a width lies between exactly two faces";
        }
        const BRepAdaptor_Surface first(faces.face(target.component, target.faces[0]));
        const BRepAdaptor_Surface second(faces.face(target.component, target.faces[1]));
        if (first.GetType() != GeomAbs_Plane || second.GetType() != GeomAbs_Plane
            || !first.Plane().Axis().IsParallel(second.Plane().Axis(), 1e-9)) {
            return "a width lies between two parallel planar faces";
        }
        return nullptr;
    }
    return "unknown size kind";
}

// Labels of the target's faces under its component's part.
TDF_LabelSequence face_labels(
    const Handle(XCAFDoc_ShapeTool)& shape_tool,
    const TDF_Label& part,
    ComponentFaces& faces,
    const occt_bridge_step_pmi_faces_t& target,
    size_t first = 0,
    size_t count = SIZE_MAX) {
    TDF_LabelSequence labels;
    const size_t end = std::min(target.face_count, first + std::min(count, target.face_count));
    for (size_t index = first; index < end; ++index) {
        const TopoDS_Face& face = faces.face(target.component, target.faces[index]);
        TDF_Label label;
        if (!shape_tool->FindSubShape(part, face, label)) {
            label = shape_tool->AddSubShape(part, face);
        }
        if (!label.IsNull()) {
            labels.Append(label);
        }
    }
    return labels;
}

Handle(TCollection_HAsciiString) text(const char* value) {
    return new TCollection_HAsciiString(value);
}

// Datum modifiers of a reference, ascending.
std::vector<int> datum_modifiers(const occt_bridge_step_datum_reference_t& reference) {
    std::vector<int> modifiers;
    if (reference.boundary == OCCT_BRIDGE_DATUM_MMB) {
        modifiers.push_back(XCAFDimTolObjects_DatumSingleModif_MaximumMaterialRequirement);
    } else if (reference.boundary == OCCT_BRIDGE_DATUM_LMB) {
        modifiers.push_back(XCAFDimTolObjects_DatumSingleModif_LeastMaterialRequirement);
    }
    if (reference.translation == 1) {
        modifiers.push_back(XCAFDimTolObjects_DatumSingleModif_Translation);
    }
    std::sort(modifiers.begin(), modifiers.end());
    return modifiers;
}

// Tolerance modifiers of the bits, ascending.
std::vector<int> tolerance_modifiers(uint32_t bits) {
    std::vector<int> modifiers;
    if ((bits & OCCT_BRIDGE_GDT_FREE_STATE) != 0) {
        modifiers.push_back(XCAFDimTolObjects_GeomToleranceModif_Free_State);
    }
    if ((bits & OCCT_BRIDGE_GDT_STATISTICAL) != 0) {
        modifiers.push_back(XCAFDimTolObjects_GeomToleranceModif_Statistical_Tolerance);
    }
    if ((bits & OCCT_BRIDGE_GDT_TANGENT_PLANE) != 0) {
        modifiers.push_back(XCAFDimTolObjects_GeomToleranceModif_Tangent_Plane);
    }
    std::sort(modifiers.begin(), modifiers.end());
    return modifiers;
}

bool finite3(const double value[3]) {
    return std::isfinite(value[0]) && std::isfinite(value[1]) && std::isfinite(value[2]);
}

gp_XYZ xyz(const double value[3]) {
    return gp_XYZ(value[0], value[1], value[2]);
}

// A drawn segment of a presentation polyline.
struct Segment {
    gp_XYZ start;
    gp_XYZ end;
};

// The segments of a presentation's polylines, without those too short to
// make an edge.
std::vector<Segment> segments(const occt_bridge_step_presentation_t& presentation) {
    std::vector<Segment> result;
    size_t first = 0;
    for (size_t polyline = 0; polyline < presentation.polyline_count; ++polyline) {
        const size_t count = presentation.polyline_points[polyline];
        for (size_t index = first + 1; index < first + count; ++index) {
            const gp_XYZ start = xyz(presentation.points + 3 * (index - 1));
            const gp_XYZ end = xyz(presentation.points + 3 * index);
            if ((end - start).Modulus() > 1e-6) {
                result.push_back({start, end});
            }
        }
        first += count;
    }
    return result;
}

const char* presentation_error(const occt_bridge_step_presentation_t& presentation) {
    if (presentation.name == nullptr || presentation.name[0] == '\0') {
        return "presentations need nonempty names";
    }
    if (presentation.points == nullptr || presentation.polyline_points == nullptr || presentation.polyline_count == 0) {
        return "presentations need polylines";
    }
    size_t total = 0;
    for (size_t polyline = 0; polyline < presentation.polyline_count; ++polyline) {
        if (presentation.polyline_points[polyline] < 2) {
            return "presentation polylines need at least two points";
        }
        total += presentation.polyline_points[polyline];
    }
    if (total != presentation.point_count) {
        return "presentation polylines must use every point once";
    }
    for (size_t index = 0; index < presentation.point_count; ++index) {
        if (!finite3(presentation.points + 3 * index)) {
            return "presentation points must be finite";
        }
    }
    if (!finite3(presentation.origin) || !finite3(presentation.normal) || !finite3(presentation.x_direction)) {
        return "an annotation plane must be finite";
    }
    const gp_XYZ normal = xyz(presentation.normal);
    const gp_XYZ x = xyz(presentation.x_direction);
    if (std::abs(normal.Modulus() - 1.0) > 1e-9 || std::abs(x.Modulus() - 1.0) > 1e-9
        || std::abs(normal.Dot(x)) > 1e-9) {
        return "an annotation plane needs a unit normal and a unit x direction in the plane";
    }
    if (segments(presentation).empty()) {
        return "a presentation needs a segment at least 1e-6 mm long";
    }
    return nullptr;
}

// The presentation as line edges, one per segment, which OCCT's writer
// tessellates point for point.
TopoDS_Compound presentation_shape(const occt_bridge_step_presentation_t& presentation) {
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    for (const Segment& segment : segments(presentation)) {
        builder.Add(compound, BRepBuilderAPI_MakeEdge(gp_Pnt(segment.start), gp_Pnt(segment.end)).Edge());
    }
    return compound;
}

template <typename Object>
void present(const Handle(Object)& object, const occt_bridge_step_presentation_t* presentation) {
    if (presentation == nullptr) {
        return;
    }
    object->SetPresentation(presentation_shape(*presentation), text(presentation->name));
    object->SetPlane(gp_Ax2(
        gp_Pnt(xyz(presentation->origin)), gp_Dir(xyz(presentation->normal)), gp_Dir(xyz(presentation->x_direction))));
    object->SetPointTextAttach(gp_Pnt(xyz(presentation->origin)));
}

TDF_Label add_datum(
    const Handle(XCAFDoc_DimTolTool)& tool,
    const TDF_LabelSequence& faces,
    const occt_bridge_step_datum_t& written,
    int position,
    const std::vector<int>& modifiers) {
    const TDF_Label datum = tool->AddDatum();
    Handle(XCAFDimTolObjects_DatumObject) object = new XCAFDimTolObjects_DatumObject;
    object->SetName(text(written.label));
    // OCCT writes the presentation of a label's first datum only.
    present(object, written.presentation);
    if (position > 0) {
        object->SetPosition(position);
    }
    for (const int modifier : modifiers) {
        object->AddModifier(static_cast<XCAFDimTolObjects_DatumSingleModif>(modifier));
    }
    XCAFDoc_Datum::Set(datum)->SetObject(object);
    tool->SetDatum(faces, datum);
    return datum;
}

// A face's part-local area and centroid, which identify it across a write
// and read.
struct Print {
    double area;
    gp_XYZ center;
};

Print print(const TopoDS_Shape& face) {
    GProp_GProps properties;
    BRepGProp::SurfaceProperties(face, properties);
    return {properties.Mass(), properties.CentreOfMass().XYZ()};
}

bool close(double first, double second) {
    return std::abs(first - second) <= 1e-6 * (1.0 + std::max(std::abs(first), std::abs(second)));
}

bool same(const Print& first, const Print& second) {
    return close(first.area, second.area) && close(first.center.X(), second.center.X())
        && close(first.center.Y(), second.center.Y()) && close(first.center.Z(), second.center.Z());
}

// Face sets sorted by centroid x.
using Prints = std::vector<Print>;

void sort_prints(Prints& prints) {
    std::sort(prints.begin(), prints.end(), [](const Print& a, const Print& b) { return a.center.X() < b.center.X(); });
}

// Whether two face sets sorted by centroid x match one to one. Each face
// searches only the read faces whose x lies within tolerance.
bool same_faces(const Prints& written, const Prints& read) {
    if (written.size() != read.size()) {
        return false;
    }
    std::vector<bool> used(read.size(), false);
    for (const Print& face : written) {
        const double reach = 2e-6 * (1.0 + std::abs(face.center.X()));
        auto it = std::lower_bound(read.begin(), read.end(), face.center.X() - reach, [](const Print& p, double x) {
            return p.center.X() < x;
        });
        bool found = false;
        for (; it != read.end() && it->center.X() <= face.center.X() + reach; ++it) {
            const size_t index = static_cast<size_t>(it - read.begin());
            if (!used[index] && same(face, *it)) {
                used[index] = true;
                found = true;
                break;
            }
        }
        if (!found) {
            return false;
        }
    }
    return true;
}

double mean_x(const Prints& prints) {
    double sum = 0.0;
    for (const Print& p : prints) {
        sum += p.center.X();
    }
    return prints.empty() ? 0.0 : sum / static_cast<double>(prints.size());
}

Prints written_prints(ComponentFaces& faces, const occt_bridge_step_pmi_faces_t& target, size_t first, size_t count) {
    Prints prints;
    const size_t end = std::min(target.face_count, first + count);
    for (size_t index = first; index < end; ++index) {
        prints.push_back(print(faces.face(target.component, target.faces[index])));
    }
    sort_prints(prints);
    return prints;
}

// Measurements of read faces, each taken once: datum faces recur in every
// tolerance that references them.
using PrintCache = std::unordered_map<TopoDS_Shape, Print>;

Prints read_prints(
    const Handle(XCAFDoc_ShapeTool)& shape_tool,
    const TDF_LabelSequence& labels,
    PrintCache& cache) {
    Prints prints;
    for (const TDF_Label& label : labels) {
        const TopoDS_Shape shape = shape_tool->GetShape(label);
        if (!shape.IsNull() && shape.ShapeType() == TopAbs_FACE) {
            auto found = cache.find(shape);
            if (found == cache.end()) {
                found = cache.emplace(shape, print(shape)).first;
            }
            prints.push_back(found->second);
        } else {
            // A shape attachment that is not a face never matches a face set.
            prints.push_back({-1.0, gp_XYZ()});
        }
    }
    sort_prints(prints);
    return prints;
}

std::string name_of(const Handle(TCollection_HAsciiString)& name) {
    return name.IsNull() ? std::string() : std::string(name->ToCString());
}


// A datum reference: label, position and ascending modifiers, with the
// datum's faces.
using Reference = std::tuple<std::string, int, std::vector<int>>;
using Referenced = std::pair<Reference, Prints>;

bool same_references(std::vector<Referenced> written, const std::vector<Referenced>& read) {
    if (written.size() != read.size()) {
        return false;
    }
    std::sort(written.begin(), written.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
    for (size_t index = 0; index < written.size(); ++index) {
        if (written[index].first != read[index].first || !same_faces(written[index].second, read[index].second)) {
            return false;
        }
    }
    return true;
}

struct ReadTolerance {
    int type;
    double value;
    int type_of_value;
    int material;
    std::vector<int> modifiers;
    int zone;
    double zone_value;
    std::vector<Referenced> datums;
    Prints faces;
    double x;
    bool claimed = false;
};

struct ReadSize {
    int type;
    double lower;
    double upper;
    Prints first;
    Prints second;
    double x;
    bool claimed = false;
};

// The face sets of each datum label in the read file. OCCT's reader makes
// datum labels only for datums that tolerances reference, so datums are read
// from the entities: a DATUM related to its DATUM_FEATURE, whose faces (and
// those of shape aspects it is composed of) a geometric item specific usage
// identifies. O(entities) plus the faces found.
std::multimap<std::string, Prints> read_datums(const STEPControl_Reader& reader) {
    const Handle(XSControl_WorkSession)& session = reader.WS();
    const Handle(Interface_InterfaceModel) model = session->Model();
    const Handle(Transfer_TransientProcess) process = session->TransferReader()->TransientProcess();
    std::map<const Standard_Transient*, std::vector<Handle(StepRepr_ShapeAspect)>> parts;
    std::map<const Standard_Transient*, Prints> faces;
    std::vector<std::pair<std::string, Handle(StepRepr_ShapeAspect)>> datums;
    for (int index = 1; index <= model->NbEntities(); ++index) {
        const Handle(Standard_Transient)& entity = model->Value(index);
        if (const auto relation = Handle(StepRepr_ShapeAspectRelationship)::DownCast(entity); !relation.IsNull()) {
            const auto datum = Handle(StepDimTol_Datum)::DownCast(relation->RelatedShapeAspect());
            if (!datum.IsNull()) {
                datums.emplace_back(name_of(datum->Identification()), relation->RelatingShapeAspect());
            } else if (!relation->RelatingShapeAspect().IsNull()) {
                parts[relation->RelatingShapeAspect().get()].push_back(relation->RelatedShapeAspect());
            }
        } else if (const auto usage = Handle(StepAP242_GeometricItemSpecificUsage)::DownCast(entity);
                   !usage.IsNull()) {
            Prints& found = faces[usage->Definition().Value().get()];
            for (int item = 1; item <= usage->NbIdentifiedItem(); ++item) {
                const TopoDS_Shape shape = TransferBRep::ShapeResult(process, usage->IdentifiedItemValue(item));
                found.push_back(!shape.IsNull() && shape.ShapeType() == TopAbs_FACE ? print(shape) : Print{-1.0, gp_XYZ()});
            }
        }
    }
    std::multimap<std::string, Prints> result;
    for (const auto& [label, feature] : datums) {
        Prints all;
        std::vector<const Standard_Transient*> pending{feature.get()};
        std::set<const Standard_Transient*> seen;
        while (!pending.empty()) {
            const Standard_Transient* aspect = pending.back();
            pending.pop_back();
            if (aspect == nullptr || !seen.insert(aspect).second) {
                continue;
            }
            if (const auto found = faces.find(aspect); found != faces.end()) {
                all.insert(all.end(), found->second.begin(), found->second.end());
            }
            if (const auto found = parts.find(aspect); found != parts.end()) {
                for (const auto& part : found->second) {
                    pending.push_back(part.get());
                }
            }
        }
        sort_prints(all);
        result.emplace(label, std::move(all));
    }
    return result;
}

std::vector<ReadTolerance> read_tolerances(
    const Handle(XCAFDoc_DimTolTool)& tool,
    const Handle(XCAFDoc_ShapeTool)& shapes,
    PrintCache& cache) {
    std::vector<ReadTolerance> tolerances;
    TDF_LabelSequence labels;
    tool->GetGeomToleranceLabels(labels);
    for (const TDF_Label& label : labels) {
        Handle(XCAFDoc_GeomTolerance) attribute;
        if (!label.FindAttribute(XCAFDoc_GeomTolerance::GetID(), attribute) || attribute->GetObject().IsNull()) {
            continue;
        }
        const auto object = attribute->GetObject();
        ReadTolerance read{
            object->GetType(),
            object->GetValue(),
            object->GetTypeOfValue(),
            object->GetMaterialRequirementModifier(),
            {},
            object->GetZoneModifier(),
            object->GetValueOfZoneModifier(),
            {},
            {},
            0.0};
        for (const auto modifier : object->GetModifiers()) {
            read.modifiers.push_back(modifier);
        }
        std::sort(read.modifiers.begin(), read.modifiers.end());
        TDF_LabelSequence datums;
        tool->GetDatumWithObjectOfTolerLabels(label, datums);
        for (const TDF_Label& datum : datums) {
            Handle(XCAFDoc_Datum) found;
            if (!datum.FindAttribute(XCAFDoc_Datum::GetID(), found) || found->GetObject().IsNull()) {
                continue;
            }
            const auto datum_object = found->GetObject();
            std::vector<int> modifiers;
            for (const auto modifier : datum_object->GetModifiers()) {
                modifiers.push_back(modifier);
            }
            std::sort(modifiers.begin(), modifiers.end());
            TDF_LabelSequence first;
            TDF_LabelSequence second;
            tool->GetRefShapeLabel(datum, first, second);
            read.datums.emplace_back(
                Reference(name_of(datum_object->GetName()), datum_object->GetPosition(), modifiers),
                read_prints(shapes, first, cache));
        }
        std::sort(read.datums.begin(), read.datums.end(), [](const auto& a, const auto& b) {
            return a.first < b.first;
        });
        TDF_LabelSequence first;
        TDF_LabelSequence second;
        tool->GetRefShapeLabel(label, first, second);
        read.faces = read_prints(shapes, first, cache);
        read.x = mean_x(read.faces);
        tolerances.push_back(std::move(read));
    }
    std::sort(tolerances.begin(), tolerances.end(), [](const ReadTolerance& a, const ReadTolerance& b) {
        return a.x < b.x;
    });
    return tolerances;
}

std::vector<ReadSize> read_sizes(
    const Handle(XCAFDoc_DimTolTool)& tool,
    const Handle(XCAFDoc_ShapeTool)& shapes,
    PrintCache& cache) {
    std::vector<ReadSize> sizes;
    TDF_LabelSequence labels;
    tool->GetDimensionLabels(labels);
    for (const TDF_Label& label : labels) {
        Handle(XCAFDoc_Dimension) attribute;
        if (!label.FindAttribute(XCAFDoc_Dimension::GetID(), attribute) || attribute->GetObject().IsNull()) {
            continue;
        }
        const auto object = attribute->GetObject();
        TDF_LabelSequence first;
        TDF_LabelSequence second;
        tool->GetRefShapeLabel(label, first, second);
        const bool range = object->IsDimWithRange();
        ReadSize read{
            object->GetType(),
            range ? object->GetLowerBound() : object->GetValue(),
            range ? object->GetUpperBound() : object->GetValue(),
            read_prints(shapes, first, cache),
            read_prints(shapes, second, cache),
            0.0};
        Prints all = read.first;
        all.insert(all.end(), read.second.begin(), read.second.end());
        read.x = mean_x(all);
        sizes.push_back(std::move(read));
    }
    std::sort(sizes.begin(), sizes.end(), [](const ReadSize& a, const ReadSize& b) { return a.x < b.x; });
    return sizes;
}

// Read items whose mean face x lies within tolerance of x, in a sorted list.
template <typename Item>
std::pair<size_t, size_t> window(const std::vector<Item>& items, double x) {
    const double reach = 2e-6 * (1.0 + std::abs(x));
    const auto begin = std::lower_bound(items.begin(), items.end(), x - reach, [](const Item& item, double value) {
        return item.x < value;
    });
    const auto end = std::upper_bound(begin, items.end(), x + reach, [](double value, const Item& item) {
        return value < item.x;
    });
    return {static_cast<size_t>(begin - items.begin()), static_cast<size_t>(end - items.begin())};
}

uint32_t tolerance_losses(
    const occt_bridge_step_tolerance_t& tolerance,
    const occt_bridge_step_pmi_t& pmi,
    const std::vector<Prints>& datum_prints,
    const ReadTolerance& read) {
    uint32_t losses = 0;
    if (!close(read.value, tolerance.value_mm)) {
        losses |= OCCT_BRIDGE_PMI_LOST_VALUE;
    }
    const int zone = tolerance.diameter == 1 ? XCAFDimTolObjects_GeomToleranceTypeValue_Diameter
                                             : XCAFDimTolObjects_GeomToleranceTypeValue_None;
    const bool projected = tolerance.projected_mm > 0.0;
    const bool read_projected = read.zone == XCAFDimTolObjects_GeomToleranceZoneModif_Projected;
    if (read.type_of_value != zone || (!read_projected && read.zone != XCAFDimTolObjects_GeomToleranceZoneModif_None)) {
        losses |= OCCT_BRIDGE_PMI_LOST_ZONE;
    }
    const int material = tolerance.material == OCCT_BRIDGE_MATERIAL_MMC ? XCAFDimTolObjects_GeomToleranceMatReqModif_M
        : tolerance.material == OCCT_BRIDGE_MATERIAL_LMC                ? XCAFDimTolObjects_GeomToleranceMatReqModif_L
                                                                        : XCAFDimTolObjects_GeomToleranceMatReqModif_None;
    if (read.material != material) {
        losses |= OCCT_BRIDGE_PMI_LOST_MATERIAL;
    }
    if (read.modifiers != tolerance_modifiers(tolerance.modifiers)) {
        losses |= OCCT_BRIDGE_PMI_LOST_MODIFIERS;
    }
    if (projected != read_projected || (projected && !close(read.zone_value, tolerance.projected_mm))) {
        losses |= OCCT_BRIDGE_PMI_LOST_PROJECTED;
    }
    std::vector<Referenced> datums;
    for (size_t index = 0; index < tolerance.datum_count; ++index) {
        const auto& reference = tolerance.datums[index];
        datums.emplace_back(
            Reference(
                pmi.datums[reference.datum].label,
                static_cast<int>(reference.compartment) + 1,
                datum_modifiers(reference)),
            datum_prints[reference.datum]);
    }
    if (!same_references(std::move(datums), read.datums)) {
        losses |= OCCT_BRIDGE_PMI_LOST_DATUMS;
    }
    return losses;
}

const char* view_error(const occt_bridge_step_view_t& view, const occt_bridge_step_pmi_t& pmi) {
    if (view.name == nullptr || view.name[0] == '\0') {
        return "views need nonempty names";
    }
    if (!finite3(view.direction) || !finite3(view.up)) {
        return "a view's direction and up must be finite";
    }
    const gp_XYZ direction = xyz(view.direction);
    const gp_XYZ up = xyz(view.up);
    if (direction.Modulus() <= 1e-12 || up.Modulus() <= 1e-12
        || direction.Crossed(up).Modulus() <= 1e-9 * direction.Modulus() * up.Modulus()) {
        return "a view's direction and up must be nonzero and not parallel";
    }
    if ((view.datum_count != 0 && view.datums == nullptr) || (view.tolerance_count != 0 && view.tolerances == nullptr)) {
        return "view annotation arrays are missing";
    }
    if (view.clipping_plane_count > 16 || (view.clipping_plane_count != 0 && view.clipping_planes == nullptr)) {
        return "a view has at most 16 clipping planes";
    }
    for (size_t index = 0; index < view.clipping_plane_count; ++index) {
        const auto& plane = view.clipping_planes[index];
        if (!finite3(plane.origin) || !finite3(plane.normal) || std::abs(xyz(plane.normal).Modulus() - 1.0) > 1e-9) {
            return "a clipping plane needs a finite origin and a unit normal";
        }
    }
    std::set<size_t> seen;
    for (size_t index = 0; index < view.datum_count; ++index) {
        if (view.datums[index] >= pmi.datum_count) {
            return "a view shows an unknown datum";
        }
        if (pmi.datums[view.datums[index]].presentation == nullptr) {
            return "a view shows only annotations with presentations";
        }
        if (!seen.insert(view.datums[index]).second) {
            return "a view shows an annotation at most once";
        }
    }
    seen.clear();
    for (size_t index = 0; index < view.tolerance_count; ++index) {
        if (view.tolerances[index] >= pmi.tolerance_count) {
            return "a view shows an unknown tolerance";
        }
        if (pmi.tolerances[view.tolerances[index]].presentation == nullptr) {
            return "a view shows only annotations with presentations";
        }
        if (!seen.insert(view.tolerances[index]).second) {
            return "a view shows an annotation at most once";
        }
    }
    return nullptr;
}

// The view's look direction and its up made square to it, both unit.
std::pair<gp_Dir, gp_Dir> camera_axes(const occt_bridge_step_view_t& view) {
    const gp_Dir direction(xyz(view.direction));
    const gp_XYZ up = xyz(view.up);
    return {direction, gp_Dir(up - direction.XYZ() * up.Dot(direction.XYZ()))};
}

Handle(StepGeom_CartesianPoint) step_point(const gp_XYZ& value) {
    Handle(TColStd_HArray1OfReal) coordinates = new TColStd_HArray1OfReal(1, 3);
    for (int index = 1; index <= 3; ++index) {
        coordinates->SetValue(index, value.Coord(index));
    }
    Handle(StepGeom_CartesianPoint) point = new StepGeom_CartesianPoint;
    point->Init(text(""), coordinates);
    return point;
}

Handle(StepGeom_Direction) step_direction(const gp_Dir& value) {
    Handle(TColStd_HArray1OfReal) ratios = new TColStd_HArray1OfReal(1, 3);
    for (int index = 1; index <= 3; ++index) {
        ratios->SetValue(index, value.Coord(index));
    }
    Handle(StepGeom_Direction) direction = new StepGeom_Direction;
    direction->Init(text(""), ratios);
    return direction;
}

// A parallel camera looking along the view's direction at the center of
// bounds, framing a sphere around them. OCCT reads the view direction as the
// placement's axis and up as the axis crossed with its x direction.
Handle(StepVisual_CameraModelD3) camera(const occt_bridge_step_view_t& view, const Bnd_Box& bounds) {
    gp_XYZ center(0.0, 0.0, 0.0);
    double radius = 1.0;
    if (!bounds.IsVoid()) {
        const gp_XYZ low = bounds.CornerMin().XYZ();
        const gp_XYZ high = bounds.CornerMax().XYZ();
        center = 0.5 * (low + high);
        radius = std::max(radius, 0.5 * (high - low).Modulus());
    }
    const auto [direction, up] = camera_axes(view);
    const gp_XYZ eye = center - 2.0 * radius * direction.XYZ();
    Handle(StepGeom_Axis2Placement3d) system = new StepGeom_Axis2Placement3d;
    system->Init(text(""), step_point(eye), Standard_True, step_direction(direction), Standard_True,
                 step_direction(gp_Dir(up.XYZ().Crossed(direction.XYZ()))));
    Handle(StepGeom_Axis2Placement3d) origin = new StepGeom_Axis2Placement3d;
    origin->Init(text(""), step_point(gp_XYZ(0.0, 0.0, 0.0)), Standard_False, nullptr, Standard_False, nullptr);
    StepGeom_Axis2Placement placement;
    placement.SetValue(origin);
    Handle(StepVisual_PlanarBox) window = new StepVisual_PlanarBox;
    window->Init(text(""), 2.0 * radius, 2.0 * radius, placement);
    Handle(StepVisual_ViewVolume) volume = new StepVisual_ViewVolume;
    volume->Init(StepVisual_copParallel, step_point(eye), 2.0 * radius, 0.0, Standard_False, 4.0 * radius,
                 Standard_False, Standard_False, window);
    if (view.clipping_plane_count == 0) {
        Handle(StepVisual_CameraModelD3) result = new StepVisual_CameraModelD3;
        result->Init(text(view.name), system, volume);
        return result;
    }
    // The planes intersect: OCCT's reader reads a list of planes as their
    // intersection, each keeping the side its normal points into.
    Handle(StepVisual_HArray1OfCameraModelD3MultiClippingInterectionSelect) planes =
        new StepVisual_HArray1OfCameraModelD3MultiClippingInterectionSelect(
            1, static_cast<int>(view.clipping_plane_count));
    for (size_t index = 0; index < view.clipping_plane_count; ++index) {
        const auto& clipping = view.clipping_planes[index];
        const gp_Ax3 axes(gp_Pnt(xyz(clipping.origin)), gp_Dir(xyz(clipping.normal)));
        Handle(StepGeom_Axis2Placement3d) placement = new StepGeom_Axis2Placement3d;
        placement->Init(text(""), step_point(axes.Location().XYZ()), Standard_True, step_direction(axes.Direction()),
                        Standard_True, step_direction(axes.XDirection()));
        Handle(StepGeom_Plane) plane = new StepGeom_Plane;
        plane->Init(text((std::string(view.name) + " clipping " + std::to_string(index + 1)).c_str()), placement);
        StepVisual_CameraModelD3MultiClippingInterectionSelect select;
        select.SetValue(plane);
        planes->SetValue(static_cast<int>(index) + 1, select);
    }
    Handle(StepVisual_CameraModelD3MultiClipping) result = new StepVisual_CameraModelD3MultiClipping;
    result->Init(text(view.name), system, volume, planes);
    return result;
}

// Bounds of the placed components and the presentations; each distinct part
// is measured once.
Bnd_Box view_bounds(const occt_bridge_step_pmi_t& pmi, const std::vector<const TopoDS_Shape*>& shapes) {
    Bnd_Box bounds;
    std::unordered_map<const TopoDS_TShape*, Bnd_Box> parts;
    for (const TopoDS_Shape* shape : shapes) {
        auto found = parts.find(shape->TShape().get());
        if (found == parts.end()) {
            Bnd_Box local;
            BRepBndLib::Add(shape->Located(TopLoc_Location()), local, Standard_False);
            found = parts.emplace(shape->TShape().get(), local).first;
        }
        if (!found->second.IsVoid()) {
            bounds.Add(found->second.Transformed(shape->Location().Transformation()));
        }
    }
    const auto add = [&](const occt_bridge_step_presentation_t* presentation) {
        for (size_t index = 0; presentation != nullptr && index < presentation->point_count; ++index) {
            bounds.Add(gp_Pnt(xyz(presentation->points + 3 * index)));
        }
    };
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        add(pmi.datums[index].presentation);
    }
    for (size_t index = 0; index < pmi.tolerance_count; ++index) {
        add(pmi.tolerances[index].presentation);
    }
    return bounds;
}

// The first length unit of a representation context.
Handle(StepBasic_NamedUnit) length_unit(const Handle(StepRepr_RepresentationContext)& context) {
    Handle(StepBasic_HArray1OfNamedUnit) units;
    if (const auto complex = Handle(StepGeom_GeomRepContextAndGlobUnitAssCtxAndGlobUncertaintyAssCtx)::DownCast(context);
        !complex.IsNull()) {
        units = complex->Units();
    } else if (const auto assigned = Handle(StepRepr_GlobalUnitAssignedContext)::DownCast(context); !assigned.IsNull()) {
        units = assigned->Units();
    }
    for (int index = 1; !units.IsNull() && index <= units->Length(); ++index) {
        const Handle(StepBasic_NamedUnit)& unit = units->Value(index);
        if (unit->IsKind(STANDARD_TYPE(StepBasic_SiUnitAndLengthUnit))
            || unit->IsKind(STANDARD_TYPE(StepBasic_ConversionBasedUnitAndLengthUnit))) {
            return unit;
        }
    }
    return nullptr;
}

constexpr const char* kPresentationLink = "PMI representation to presentation link";

// The segments of a callout's tessellated curves.
std::vector<Segment> callout_segments(const Handle(StepVisual_DraughtingCallout)& callout) {
    std::vector<Segment> result;
    for (int content = 1; content <= callout->NbContents(); ++content) {
        const auto occurrence = callout->ContentsValue(content).TessellatedAnnotationOccurrence();
        if (occurrence.IsNull()) {
            continue;
        }
        const auto set = Handle(StepVisual_TessellatedGeometricSet)::DownCast(occurrence->Item());
        if (set.IsNull() || set->Items().IsNull()) {
            continue;
        }
        for (const auto& item : *set->Items()) {
            const auto curves = Handle(StepVisual_TessellatedCurveSet)::DownCast(item);
            if (curves.IsNull() || curves->CoordList().IsNull() || curves->Curves().IsNull()) {
                continue;
            }
            const Handle(TColgp_HArray1OfXYZ) points = curves->CoordList()->Points();
            for (const auto& curve : *curves->Curves()) {
                for (int index = 2; !curve.IsNull() && index <= curve->Length(); ++index) {
                    const int a = curve->Value(index - 1);
                    const int b = curve->Value(index);
                    if (points.IsNull() || a < points->Lower() || a > points->Upper() || b < points->Lower()
                        || b > points->Upper()) {
                        continue;
                    }
                    result.push_back({points->Value(a), points->Value(b)});
                }
            }
        }
    }
    return result;
}

bool same_point(const gp_XYZ& first, const gp_XYZ& second) {
    return close(first.X(), second.X()) && close(first.Y(), second.Y()) && close(first.Z(), second.Z());
}

bool same_segments(const std::vector<Segment>& written, const std::vector<Segment>& read) {
    if (written.size() != read.size()) {
        return false;
    }
    for (size_t index = 0; index < written.size(); ++index) {
        if (!same_point(written[index].start, read[index].start) || !same_point(written[index].end, read[index].end)) {
            return false;
        }
    }
    return true;
}

// What the file holds of presentations and views: each linked callout's
// segments by name, and each camera's callout names.
struct ReadPresentations {
    std::map<std::string, std::vector<std::vector<Segment>>> callouts;
    std::map<std::string, std::set<std::string>> views;
};

ReadPresentations read_presentations(const Handle(Interface_InterfaceModel)& model) {
    ReadPresentations result;
    for (int index = 1; index <= model->NbEntities(); ++index) {
        const Handle(Standard_Transient)& entity = model->Value(index);
        if (const auto link = Handle(StepAP242_DraughtingModelItemAssociation)::DownCast(entity); !link.IsNull()) {
            // OCCT's reader lowercases the names of links it has followed.
            if (link->Name().IsNull() || !link->Name()->IsSameString(text(kPresentationLink), Standard_False)
                || link->Definition().Value().IsNull()) {
                continue;
            }
            for (int item = 1; item <= link->NbIdentifiedItem(); ++item) {
                const auto callout = Handle(StepVisual_DraughtingCallout)::DownCast(link->IdentifiedItemValue(item));
                if (!callout.IsNull()) {
                    result.callouts[name_of(callout->Name())].push_back(callout_segments(callout));
                }
            }
        } else if (const auto drawing = Handle(StepVisual_DraughtingModel)::DownCast(entity); !drawing.IsNull()) {
            std::vector<std::string> cameras;
            std::set<std::string> callouts;
            for (int item = 1; item <= drawing->NbItems(); ++item) {
                const Handle(StepRepr_RepresentationItem) value = drawing->ItemsValue(item);
                if (const auto camera = Handle(StepVisual_CameraModelD3)::DownCast(value); !camera.IsNull()) {
                    cameras.push_back(name_of(camera->Name()));
                } else if (const auto callout = Handle(StepVisual_DraughtingCallout)::DownCast(value);
                           !callout.IsNull()) {
                    callouts.insert(name_of(callout->Name()));
                }
            }
            for (const std::string& camera : cameras) {
                result.views[camera].insert(callouts.begin(), callouts.end());
            }
        }
    }
    return result;
}

uint32_t presentation_losses(const occt_bridge_step_presentation_t* presentation, const ReadPresentations& read) {
    if (presentation == nullptr) {
        return 0;
    }
    const auto found = read.callouts.find(presentation->name);
    const std::vector<Segment> written = segments(*presentation);
    const bool same = found != read.callouts.end()
        && std::any_of(found->second.begin(), found->second.end(), [&](const std::vector<Segment>& callout) {
               return same_segments(written, callout);
           });
    return same ? 0u : OCCT_BRIDGE_PMI_LOST_PRESENTATION;
}

// Whether the read planes are the view's clipping planes, one to one: the
// same normal and each written origin on its plane.
bool same_planes(const occt_bridge_step_view_t& view, const std::vector<gp_Pln>& read) {
    if (read.size() != view.clipping_plane_count) {
        return false;
    }
    std::vector<bool> used(read.size(), false);
    for (size_t index = 0; index < view.clipping_plane_count; ++index) {
        const auto& written = view.clipping_planes[index];
        const gp_Dir normal(xyz(written.normal));
        const gp_Pnt origin(xyz(written.origin));
        bool found = false;
        for (size_t candidate = 0; candidate < read.size() && !found; ++candidate) {
            if (!used[candidate] && read[candidate].Axis().Direction().IsEqual(normal, 1e-7)
                && read[candidate].Distance(origin) <= 1e-6 * (1.0 + origin.XYZ().Modulus())) {
                used[candidate] = true;
                found = true;
            }
        }
        if (!found) {
            return false;
        }
    }
    return true;
}

bool nonempty(const char* text) {
    return text != nullptr && text[0] != '\0';
}

// A YYYY-MM-DD calendar date as {year, month, day}, or nothing.
std::optional<std::array<int, 3>> iso_date(const char* text) {
    if (text == nullptr || std::strlen(text) != 10 || text[4] != '-' || text[7] != '-') {
        return std::nullopt;
    }
    for (const int index : {0, 1, 2, 3, 5, 6, 8, 9}) {
        if (text[index] < '0' || text[index] > '9') {
            return std::nullopt;
        }
    }
    const int year = std::atoi(text);
    const int month = std::atoi(text + 5);
    const int day = std::atoi(text + 8);
    const bool leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    static constexpr int kDays[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    if (year < 1 || month < 1 || month > 12 || day < 1 || day > kDays[month - 1] + (month == 2 && leap ? 1 : 0)) {
        return std::nullopt;
    }
    return std::array<int, 3>{year, month, day};
}

const char* dataset_error(const occt_bridge_step_dataset_t& dataset) {
    if (!nonempty(dataset.identifier) || !nonempty(dataset.organization) || !nonempty(dataset.authority)) {
        return "a dataset needs an identifier, an organization and an authority";
    }
    if (!nonempty(dataset.revision) && dataset.approval_count != 0) {
        return "an unreleased dataset has no approvals";
    }
    if (dataset.description != nullptr && dataset.description[0] == '\0') {
        return "a revision description must be nonempty when given";
    }
    if (dataset.approval_count != 0 && dataset.approvals == nullptr) {
        return "dataset approvals are missing";
    }
    for (size_t index = 0; index < dataset.approval_count; ++index) {
        const auto& approval = dataset.approvals[index];
        if (!nonempty(approval.role) || !nonempty(approval.name) || !iso_date(approval.date)) {
            return "approvals need a role, a name and a YYYY-MM-DD date";
        }
    }
    return nullptr;
}

// The product definition no assembly uses: the export's root, or null when
// there is not exactly one.
Handle(StepBasic_ProductDefinition) root_definition(const Handle(Interface_InterfaceModel)& model) {
    std::set<const Standard_Transient*> used;
    std::vector<Handle(StepBasic_ProductDefinition)> definitions;
    for (int index = 1; index <= model->NbEntities(); ++index) {
        const Handle(Standard_Transient)& entity = model->Value(index);
        if (const auto usage = Handle(StepRepr_NextAssemblyUsageOccurrence)::DownCast(entity); !usage.IsNull()) {
            used.insert(usage->RelatedProductDefinition().get());
        } else if (const auto definition = Handle(StepBasic_ProductDefinition)::DownCast(entity);
                   !definition.IsNull()) {
            definitions.push_back(definition);
        }
    }
    Handle(StepBasic_ProductDefinition) root;
    for (const auto& definition : definitions) {
        if (used.count(definition.get()) == 0) {
            if (!root.IsNull()) {
                return nullptr;
            }
            root = definition;
        }
    }
    return root;
}

// Identification and approvals of the dataset on the root product, as
// AP242 product data management: the product's ID, the formation's ID and
// description for the revision, the definition's description for the
// authority, and per approval an approved APPROVAL with its date, approver
// and role, assigned to the formation.
void write_dataset(const Handle(Interface_InterfaceModel)& model, const occt_bridge_step_dataset_t& dataset) {
    const Handle(StepBasic_ProductDefinition) root = root_definition(model);
    if (root.IsNull() || root->Formation().IsNull() || root->Formation()->OfProduct().IsNull()) {
        return;
    }
    const Handle(StepBasic_ProductDefinitionFormation) formation = root->Formation();
    formation->OfProduct()->SetId(text(dataset.identifier));
    root->SetDescription(text(dataset.authority));
    if (!nonempty(dataset.revision)) {
        return;
    }
    formation->SetId(text(dataset.revision));
    if (dataset.description != nullptr) {
        formation->SetDescription(text(dataset.description));
    }
    Handle(StepBasic_Organization) organization = new StepBasic_Organization;
    organization->Init(Standard_False, nullptr, text(dataset.organization), text(""));
    std::map<std::string, Handle(StepBasic_PersonAndOrganization)> people;
    for (size_t index = 0; index < dataset.approval_count; ++index) {
        const auto& entry = dataset.approvals[index];
        auto& person = people[entry.name];
        if (person.IsNull()) {
            Handle(StepBasic_Person) who = new StepBasic_Person;
            who->Init(text(entry.name), Standard_True, text(entry.name), Standard_False, nullptr, Standard_False,
                      nullptr, Standard_False, nullptr, Standard_False, nullptr);
            person = new StepBasic_PersonAndOrganization;
            person->Init(who, organization);
        }
        Handle(StepBasic_ApprovalStatus) status = new StepBasic_ApprovalStatus;
        status->Init(text("approved"));
        Handle(StepBasic_Approval) approval = new StepBasic_Approval;
        approval->Init(status, text(entry.role));
        const auto [year, month, day] = *iso_date(entry.date);
        Handle(StepBasic_CalendarDate) date = new StepBasic_CalendarDate;
        date->Init(year, day, month);
        StepBasic_DateTimeSelect when;
        when.SetValue(date);
        Handle(StepBasic_ApprovalDateTime) dated = new StepBasic_ApprovalDateTime;
        dated->Init(when, approval);
        Handle(StepBasic_ApprovalRole) role = new StepBasic_ApprovalRole;
        role->Init(text(entry.role));
        StepBasic_PersonOrganizationSelect signer;
        signer.SetValue(person);
        Handle(StepBasic_ApprovalPersonOrganization) signature = new StepBasic_ApprovalPersonOrganization;
        signature->Init(signer, approval, role);
        Handle(StepAP214_HArray1OfApprovalItem) items = new StepAP214_HArray1OfApprovalItem(1, 1);
        StepAP214_ApprovalItem item;
        item.SetValue(formation);
        items->SetValue(1, item);
        Handle(StepAP214_AppliedApprovalAssignment) assignment = new StepAP214_AppliedApprovalAssignment;
        assignment->Init(approval, items);
        model->AddWithRefs(dated);
        model->AddWithRefs(signature);
        model->AddWithRefs(assignment);
    }
}

// An approval as read: role, approver and date.
using ReadApproval = std::tuple<std::string, std::string, std::array<int, 3>>;

uint32_t dataset_losses(const Handle(Interface_InterfaceModel)& model, const occt_bridge_step_dataset_t& dataset) {
    const Handle(StepBasic_ProductDefinition) root = root_definition(model);
    if (root.IsNull() || root->Formation().IsNull() || root->Formation()->OfProduct().IsNull()) {
        return OCCT_BRIDGE_DATASET_LOST_IDENTIFIER | OCCT_BRIDGE_DATASET_LOST_REVISION
            | OCCT_BRIDGE_DATASET_LOST_AUTHORITY | OCCT_BRIDGE_DATASET_LOST_APPROVALS;
    }
    const Handle(StepBasic_ProductDefinitionFormation) formation = root->Formation();
    uint32_t losses = 0;
    if (name_of(formation->OfProduct()->Id()) != dataset.identifier) {
        losses |= OCCT_BRIDGE_DATASET_LOST_IDENTIFIER;
    }
    if (name_of(root->Description()) != dataset.authority) {
        losses |= OCCT_BRIDGE_DATASET_LOST_AUTHORITY;
    }
    const bool released = nonempty(dataset.revision);
    if (released
        && (name_of(formation->Id()) != dataset.revision
            || (dataset.description != nullptr && name_of(formation->Description()) != dataset.description))) {
        losses |= OCCT_BRIDGE_DATASET_LOST_REVISION;
    }
    // Approvals assigned to the root formation, with their dates and signers.
    std::set<const Standard_Transient*> assigned;
    std::map<const Standard_Transient*, std::array<int, 3>> dates;
    std::multimap<const Standard_Transient*, std::pair<std::string, std::string>> signers;
    for (int index = 1; index <= model->NbEntities(); ++index) {
        const Handle(Standard_Transient)& entity = model->Value(index);
        if (const auto assignment = Handle(StepAP214_AppliedApprovalAssignment)::DownCast(entity);
            !assignment.IsNull()) {
            for (int item = 1; item <= assignment->NbItems(); ++item) {
                if (assignment->ItemsValue(item).Value() == formation) {
                    assigned.insert(assignment->AssignedApproval().get());
                }
            }
        } else if (const auto dated = Handle(StepBasic_ApprovalDateTime)::DownCast(entity); !dated.IsNull()) {
            const auto date = Handle(StepBasic_CalendarDate)::DownCast(dated->DateTime().Value());
            if (!date.IsNull()) {
                dates[dated->DatedApproval().get()] =
                    std::array<int, 3>{date->YearComponent(), date->MonthComponent(), date->DayComponent()};
            }
        } else if (const auto signature = Handle(StepBasic_ApprovalPersonOrganization)::DownCast(entity);
                   !signature.IsNull()) {
            const auto person = signature->PersonOrganization().PersonAndOrganization();
            const std::string who = person.IsNull() || person->ThePerson().IsNull()
                ? std::string()
                : name_of(person->ThePerson()->LastName());
            const std::string role = signature->Role().IsNull() ? std::string() : name_of(signature->Role()->Role());
            signers.emplace(signature->AuthorizedApproval().get(), std::make_pair(role, who));
        }
    }
    std::vector<ReadApproval> read;
    for (int index = 1; index <= model->NbEntities(); ++index) {
        const auto approval = Handle(StepBasic_Approval)::DownCast(model->Value(index));
        if (approval.IsNull() || assigned.count(approval.get()) == 0) {
            continue;
        }
        if (approval->Status().IsNull() || name_of(approval->Status()->Name()) != "approved") {
            read.emplace_back("", "", std::array<int, 3>{});
            continue;
        }
        const auto [begin, end] = signers.equal_range(approval.get());
        const auto date = dates.find(approval.get());
        for (auto signer = begin; signer != end; ++signer) {
            read.emplace_back(signer->second.first, signer->second.second,
                              date == dates.end() ? std::array<int, 3>{} : date->second);
        }
    }
    std::vector<ReadApproval> written;
    for (size_t index = 0; released && index < dataset.approval_count; ++index) {
        const auto& entry = dataset.approvals[index];
        written.emplace_back(entry.role, entry.name, *iso_date(entry.date));
    }
    std::sort(read.begin(), read.end());
    std::sort(written.begin(), written.end());
    if (read != written) {
        losses |= OCCT_BRIDGE_DATASET_LOST_APPROVALS;
    }
    return losses;
}

// Closes a reading document however verification ends.
struct ReadDocument {
    Handle(XCAFApp_Application) application = XCAFApp_Application::GetApplication();
    Handle(TDocStd_Document) document;
    ~ReadDocument() {
        if (!document.IsNull() && document->IsOpened()) {
            application->Close(document);
        }
    }
};

}  // namespace

const char* pmi_error(const occt_bridge_step_pmi_t& pmi, const std::vector<const TopoDS_Shape*>& shapes) {
    if ((pmi.datum_count != 0 && pmi.datums == nullptr) || (pmi.tolerance_count != 0 && pmi.tolerances == nullptr)
        || (pmi.size_count != 0 && pmi.sizes == nullptr)) {
        return "PMI arrays are missing";
    }
    ComponentFaces faces(shapes);
    std::vector<std::string> labels;
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        const auto& datum = pmi.datums[index];
        if (datum.label == nullptr || datum.label[0] == '\0') {
            return "datums need nonempty labels";
        }
        if (const char* error = faces_error(datum.target, faces, shapes.size())) {
            return error;
        }
        labels.push_back(datum.label);
    }
    std::sort(labels.begin(), labels.end());
    if (std::adjacent_find(labels.begin(), labels.end()) != labels.end()) {
        return "datum labels must be unique across the export";
    }
    for (size_t index = 0; index < pmi.tolerance_count; ++index) {
        const auto& tolerance = pmi.tolerances[index];
        if (const char* error = faces_error(tolerance.target, faces, shapes.size())) {
            return error;
        }
        if (const char* error = tolerance_error(tolerance, pmi, shapes)) {
            return error;
        }
    }
    for (size_t index = 0; index < pmi.size_count; ++index) {
        if (const char* error = faces_error(pmi.sizes[index].target, faces, shapes.size())) {
            return error;
        }
        if (const char* error = size_error(pmi.sizes[index], faces)) {
            return error;
        }
    }
    std::vector<std::string> names;
    const auto presentation = [&](const occt_bridge_step_presentation_t* value) -> const char* {
        if (value == nullptr) {
            return nullptr;
        }
        if (const char* error = presentation_error(*value)) {
            return error;
        }
        names.push_back(value->name);
        return nullptr;
    };
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        if (const char* error = presentation(pmi.datums[index].presentation)) {
            return error;
        }
    }
    for (size_t index = 0; index < pmi.tolerance_count; ++index) {
        if (const char* error = presentation(pmi.tolerances[index].presentation)) {
            return error;
        }
    }
    std::sort(names.begin(), names.end());
    if (std::adjacent_find(names.begin(), names.end()) != names.end()) {
        return "presentation names must be unique";
    }
    if (pmi.view_count != 0 && pmi.views == nullptr) {
        return "PMI views are missing";
    }
    names.clear();
    for (size_t index = 0; index < pmi.view_count; ++index) {
        if (const char* error = view_error(pmi.views[index], pmi)) {
            return error;
        }
        names.push_back(pmi.views[index].name);
    }
    std::sort(names.begin(), names.end());
    if (std::adjacent_find(names.begin(), names.end()) != names.end()) {
        return "view names must be unique";
    }
    if (pmi.dataset != nullptr) {
        if (const char* error = dataset_error(*pmi.dataset)) {
            return error;
        }
    }
    return nullptr;
}

void add_pmi(
    const Handle(TDocStd_Document)& document,
    const occt_bridge_step_pmi_t& pmi,
    const std::vector<TDF_Label>& component_parts,
    const std::vector<const TopoDS_Shape*>& shapes) {
    const Handle(XCAFDoc_ShapeTool) shape_tool = XCAFDoc_DocumentTool::ShapeTool(document->Main());
    const Handle(XCAFDoc_DimTolTool) tool = XCAFDoc_DocumentTool::DimTolTool(document->Main());
    ComponentFaces faces(shapes);
    const auto labels_of = [&](const occt_bridge_step_pmi_faces_t& target, size_t first = 0, size_t count = SIZE_MAX) {
        return face_labels(shape_tool, component_parts[target.component], faces, target, first, count);
    };
    std::vector<TDF_LabelSequence> datum_faces;
    std::vector<bool> referenced(pmi.datum_count, false);
    datum_faces.reserve(pmi.datum_count);
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        datum_faces.push_back(labels_of(pmi.datums[index].target));
    }
    for (size_t index = 0; index < pmi.tolerance_count; ++index) {
        const auto& tolerance = pmi.tolerances[index];
        const TDF_Label label = tool->AddGeomTolerance();
        Handle(XCAFDimTolObjects_GeomToleranceObject) object = new XCAFDimTolObjects_GeomToleranceObject;
        object->SetType(kTypes[tolerance.characteristic]);
        object->SetValue(tolerance.value_mm);
        if (tolerance.diameter == 1) {
            object->SetTypeOfValue(XCAFDimTolObjects_GeomToleranceTypeValue_Diameter);
        }
        if (tolerance.material == OCCT_BRIDGE_MATERIAL_MMC) {
            object->SetMaterialRequirementModifier(XCAFDimTolObjects_GeomToleranceMatReqModif_M);
        } else if (tolerance.material == OCCT_BRIDGE_MATERIAL_LMC) {
            object->SetMaterialRequirementModifier(XCAFDimTolObjects_GeomToleranceMatReqModif_L);
        }
        for (const int modifier : tolerance_modifiers(tolerance.modifiers)) {
            object->AddModifier(static_cast<XCAFDimTolObjects_GeomToleranceModif>(modifier));
        }
        if (tolerance.projected_mm > 0.0) {
            object->SetZoneModifier(XCAFDimTolObjects_GeomToleranceZoneModif_Projected);
            object->SetValueOfZoneModifier(tolerance.projected_mm);
        }
        present(object, tolerance.presentation);
        XCAFDoc_GeomTolerance::Set(label)->SetObject(object);
        tool->SetGeomTolerance(labels_of(tolerance.target), label);
        // Each reference is its own datum label carrying its compartment and
        // modifiers; the writer shares one datum entity per label text.
        for (size_t reference = 0; reference < tolerance.datum_count; ++reference) {
            const auto& entry = tolerance.datums[reference];
            referenced[entry.datum] = true;
            const TDF_Label datum = add_datum(
                tool,
                datum_faces[entry.datum],
                pmi.datums[entry.datum],
                static_cast<int>(entry.compartment) + 1,
                datum_modifiers(entry));
            tool->SetDatumToGeomTol(datum, label);
        }
    }
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        if (!referenced[index]) {
            add_datum(tool, datum_faces[index], pmi.datums[index], 0, {});
        }
    }
    for (size_t index = 0; index < pmi.size_count; ++index) {
        const auto& size = pmi.sizes[index];
        const TDF_Label label = tool->AddDimension();
        Handle(XCAFDimTolObjects_DimensionObject) object = new XCAFDimTolObjects_DimensionObject;
        object->SetLowerBound(size.lower_mm);
        object->SetUpperBound(size.upper_mm);
        if (size.kind == OCCT_BRIDGE_STEP_SIZE_DIAMETER) {
            object->SetType(XCAFDimTolObjects_DimensionType_Size_Diameter);
            XCAFDoc_Dimension::Set(label)->SetObject(object);
            tool->SetDimension(labels_of(size.target), TDF_LabelSequence(), label);
        } else {
            object->SetType(XCAFDimTolObjects_DimensionType_Location_LinearDistance);
            XCAFDoc_Dimension::Set(label)->SetObject(object);
            tool->SetDimension(labels_of(size.target, 0, 1), labels_of(size.target, 1, 1), label);
        }
    }
}

void finish_pmi(
    STEPCAFControl_Writer& writer,
    const occt_bridge_step_pmi_t& pmi,
    const std::vector<const TopoDS_Shape*>& shapes) {
    const Handle(Interface_InterfaceModel) model = writer.Writer().WS()->Model();
    Handle(StepRepr_RepresentationContext) context;
    std::vector<Handle(StepVisual_DraughtingModel)> drawings;
    std::vector<Handle(StepDimTol_GeometricTolerance)> tolerances;
    std::vector<Handle(StepDimTol_RunoutZoneDefinition)> runouts;
    std::map<std::string, Handle(StepVisual_DraughtingCallout)> callouts;
    for (int index = 1; index <= model->NbEntities(); ++index) {
        const Handle(Standard_Transient)& entity = model->Value(index);
        if (const auto shape = Handle(StepShape_ShapeRepresentation)::DownCast(entity);
            !shape.IsNull() && context.IsNull()) {
            context = shape->ContextOfItems();
        } else if (const auto drawing = Handle(StepVisual_DraughtingModel)::DownCast(entity); !drawing.IsNull()) {
            drawings.push_back(drawing);
        } else if (const auto tolerance = Handle(StepDimTol_GeometricTolerance)::DownCast(entity);
                   !tolerance.IsNull()) {
            tolerances.push_back(tolerance);
        } else if (const auto callout = Handle(StepVisual_DraughtingCallout)::DownCast(entity); !callout.IsNull()) {
            callouts.emplace(name_of(callout->Name()), callout);
        } else if (const auto runout = Handle(StepDimTol_RunoutZoneDefinition)::DownCast(entity); !runout.IsNull()) {
            runouts.push_back(runout);
        }
    }
    if (context.IsNull()) {
        return;
    }
    const Handle(StepBasic_NamedUnit) unit = length_unit(context);
    // OCCT 7.9 takes the presentation model's context and the tolerance
    // values' unit from the first size dimension it writes: without one the
    // context is missing, which fails the write, and values are in metres.
    for (const auto& drawing : drawings) {
        if (drawing->ContextOfItems().IsNull()) {
            drawing->SetContextOfItems(context);
        }
    }
    StepBasic_Unit length;
    if (!unit.IsNull()) {
        length.SetValue(unit);
        for (const auto& tolerance : tolerances) {
            if (!tolerance->Magnitude().IsNull()) {
                tolerance->Magnitude()->SetUnitComponent(length);
            }
        }
    }
    // OCCT 7.9 writes every diameter zone with a runout zone definition whose
    // angle is the zone modifier's value, and never a projected zone. None
    // of these tolerances has a runout zone, so each such definition goes:
    // a projected tolerance's (its angle is the projected height) becomes a
    // projected zone definition bounded by the toleranced feature.
    if (!runouts.empty() && !unit.IsNull()) {
        std::set<const Standard_Transient*> removed;
        std::vector<Handle(StepDimTol_ProjectedZoneDefinition)> projected;
        for (const auto& runout : runouts) {
            removed.insert(runout.get());
            const Handle(StepDimTol_RunoutZoneOrientation) orientation = runout->Orientation();
            if (orientation.IsNull() || orientation->Angle().IsNull()) {
                continue;
            }
            removed.insert(orientation.get());
            removed.insert(orientation->Angle().get());
            const double height = orientation->Angle()->ValueComponent();
            const Handle(StepDimTol_ToleranceZone) zone = runout->Zone();
            if (!(height > 0.0) || zone.IsNull() || zone->NbDefiningTolerances() == 0) {
                continue;
            }
            const auto tolerance = zone->DefiningToleranceValue(1).GeometricTolerance();
            Handle(StepRepr_ShapeAspect) feature =
                tolerance.IsNull() ? Handle(StepRepr_ShapeAspect)() : tolerance->TolerancedShapeAspect().ShapeAspect();
            if (feature.IsNull()) {
                feature = zone;
            }
            Handle(StepRepr_HArray1OfShapeAspect) boundaries = new StepRepr_HArray1OfShapeAspect(1, 1);
            boundaries->SetValue(1, feature);
            Handle(StepBasic_MeasureValueMember) value = new StepBasic_MeasureValueMember;
            value->SetName("LENGTH_MEASURE");
            value->SetReal(height);
            Handle(StepBasic_LengthMeasureWithUnit) measure = new StepBasic_LengthMeasureWithUnit;
            measure->Init(value, length);
            Handle(StepDimTol_ProjectedZoneDefinition) definition = new StepDimTol_ProjectedZoneDefinition;
            definition->Init(zone, boundaries, feature, measure);
            projected.push_back(definition);
        }
        std::vector<Handle(Standard_Transient)> kept;
        kept.reserve(static_cast<size_t>(model->NbEntities()));
        for (int index = 1; index <= model->NbEntities(); ++index) {
            if (removed.count(model->Value(index).get()) == 0) {
                kept.push_back(model->Value(index));
            }
        }
        model->ClearEntities();
        for (const auto& entity : kept) {
            model->AddEntity(entity);
        }
        // OCCT's reference walk skips a projected zone's length, so it is
        // added on its own.
        for (const auto& definition : projected) {
            model->AddWithRefs(definition->ProjectionLength());
            model->AddWithRefs(definition);
        }
        writer.Writer().WS()->ComputeGraph(Standard_True);
    }
    if (pmi.dataset != nullptr) {
        write_dataset(model, *pmi.dataset);
    }
    if (pmi.view_count == 0) {
        return;
    }
    // Each view is a draughting model holding its camera and the callouts
    // it shows, as OCCT's reader expects of a saved view.
    const Bnd_Box bounds = view_bounds(pmi, shapes);
    for (size_t index = 0; index < pmi.view_count; ++index) {
        const occt_bridge_step_view_t& view = pmi.views[index];
        std::vector<Handle(StepRepr_RepresentationItem)> items{camera(view, bounds)};
        const auto show = [&](const occt_bridge_step_presentation_t* presentation) {
            if (const auto found = callouts.find(presentation->name); found != callouts.end()) {
                items.push_back(found->second);
            }
        };
        for (size_t entry = 0; entry < view.datum_count; ++entry) {
            show(pmi.datums[view.datums[entry]].presentation);
        }
        for (size_t entry = 0; entry < view.tolerance_count; ++entry) {
            show(pmi.tolerances[view.tolerances[entry]].presentation);
        }
        Handle(StepRepr_HArray1OfRepresentationItem) array =
            new StepRepr_HArray1OfRepresentationItem(1, static_cast<int>(items.size()));
        for (size_t item = 0; item < items.size(); ++item) {
            array->SetValue(static_cast<int>(item) + 1, items[item]);
        }
        Handle(StepVisual_DraughtingModel) drawing = new StepVisual_DraughtingModel;
        drawing->Init(text(view.name), array, context);
        model->AddWithRefs(drawing);
    }
}

bool verify_pmi(const char* path, occt_bridge_step_pmi_t& pmi, const std::vector<const TopoDS_Shape*>& shapes) {
    STEPCAFControl_Reader reader;
    reader.SetGDTMode(Standard_True);
    reader.SetNameMode(Standard_True);
    reader.SetViewMode(Standard_True);
    if (reader.ReadFile(path) != IFSelect_RetDone) {
        return false;
    }
    ReadDocument read;
    read.application->NewDocument("MDTV-XCAF", read.document);
    if (!reader.Transfer(read.document)) {
        return false;
    }
    const Handle(XCAFDoc_ShapeTool) shape_tool = XCAFDoc_DocumentTool::ShapeTool(read.document->Main());
    const Handle(XCAFDoc_DimTolTool) tool = XCAFDoc_DocumentTool::DimTolTool(read.document->Main());
    ComponentFaces faces(shapes);

    const std::multimap<std::string, Prints> datums_by_name = read_datums(reader.Reader());
    const ReadPresentations presentations = read_presentations(reader.Reader().WS()->Model());
    std::vector<Prints> datum_prints;
    datum_prints.reserve(pmi.datum_count);
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        const auto& datum = pmi.datums[index];
        datum_prints.push_back(written_prints(faces, datum.target, 0, SIZE_MAX));
        const auto [begin, end] = datums_by_name.equal_range(datum.label);
        const bool found = std::any_of(begin, end, [&](const auto& entry) {
            return same_faces(datum_prints.back(), entry.second);
        });
        if (pmi.out_datum_losses != nullptr) {
            pmi.out_datum_losses[index] = found ? presentation_losses(datum.presentation, presentations)
                                                : OCCT_BRIDGE_PMI_LOST_ITEM;
        }
    }

    PrintCache cache;
    std::vector<ReadTolerance> tolerances = read_tolerances(tool, shape_tool, cache);
    for (size_t index = 0; index < pmi.tolerance_count; ++index) {
        const auto& tolerance = pmi.tolerances[index];
        const Prints written = written_prints(faces, tolerance.target, 0, SIZE_MAX);
        const auto [begin, end] = window(tolerances, mean_x(written));
        uint32_t best = OCCT_BRIDGE_PMI_LOST_ITEM;
        size_t chosen = SIZE_MAX;
        for (size_t candidate = begin; candidate < end && best != 0; ++candidate) {
            ReadTolerance& entry = tolerances[candidate];
            if (entry.claimed || entry.type != kTypes[tolerance.characteristic] || !same_faces(written, entry.faces)) {
                continue;
            }
            const uint32_t losses = tolerance_losses(tolerance, pmi, datum_prints, entry);
            if (chosen == SIZE_MAX || __builtin_popcount(losses) < __builtin_popcount(best)) {
                best = losses;
                chosen = candidate;
            }
        }
        if (chosen != SIZE_MAX) {
            tolerances[chosen].claimed = true;
            best |= presentation_losses(tolerance.presentation, presentations);
        }
        if (pmi.out_tolerance_losses != nullptr) {
            pmi.out_tolerance_losses[index] = best;
        }
    }

    std::vector<ReadSize> sizes = read_sizes(tool, shape_tool, cache);
    for (size_t index = 0; index < pmi.size_count; ++index) {
        const auto& size = pmi.sizes[index];
        const bool diameter = size.kind == OCCT_BRIDGE_STEP_SIZE_DIAMETER;
        const Prints first = written_prints(faces, size.target, 0, diameter ? SIZE_MAX : 1);
        const Prints second = diameter ? Prints() : written_prints(faces, size.target, 1, 1);
        Prints all = first;
        all.insert(all.end(), second.begin(), second.end());
        const int type = diameter ? XCAFDimTolObjects_DimensionType_Size_Diameter
                                  : XCAFDimTolObjects_DimensionType_Location_LinearDistance;
        const auto [begin, end] = window(sizes, mean_x(all));
        uint32_t best = OCCT_BRIDGE_PMI_LOST_ITEM;
        size_t chosen = SIZE_MAX;
        for (size_t candidate = begin; candidate < end && best != 0; ++candidate) {
            ReadSize& entry = sizes[candidate];
            const bool faces_match = (same_faces(first, entry.first) && same_faces(second, entry.second))
                || (!diameter && same_faces(first, entry.second) && same_faces(second, entry.first));
            if (entry.claimed || entry.type != type || !faces_match) {
                continue;
            }
            const uint32_t losses = close(entry.lower, size.lower_mm) && close(entry.upper, size.upper_mm)
                ? 0u
                : OCCT_BRIDGE_PMI_LOST_VALUE;
            if (chosen == SIZE_MAX || losses < best) {
                best = losses;
                chosen = candidate;
            }
        }
        if (chosen != SIZE_MAX) {
            sizes[chosen].claimed = true;
        }
        if (pmi.out_size_losses != nullptr) {
            pmi.out_size_losses[index] = best;
        }
    }

    // Views: the camera's name, direction and up as OCCT's reader makes the
    // view, and its callouts from the file's draughting models.
    const Handle(XCAFDoc_ViewTool) view_tool = XCAFDoc_DocumentTool::ViewTool(read.document->Main());
    TDF_LabelSequence view_labels;
    view_tool->GetViewLabels(view_labels);
    const Handle(XCAFDoc_ClippingPlaneTool) clipping_tool =
        XCAFDoc_DocumentTool::ClippingPlaneTool(read.document->Main());
    struct ReadView {
        Handle(XCAFView_Object) object;
        std::vector<gp_Pln> planes;
    };
    std::multimap<std::string, ReadView> cameras;
    for (const TDF_Label& label : view_labels) {
        Handle(XCAFDoc_View) attribute;
        if (label.FindAttribute(XCAFDoc_View::GetID(), attribute) && !attribute->GetObject().IsNull()) {
            ReadView entry{attribute->GetObject(), {}};
            TDF_LabelSequence planes;
            view_tool->GetRefClippingPlaneLabel(label, planes);
            for (const TDF_Label& plane_label : planes) {
                gp_Pln plane;
                TCollection_ExtendedString plane_name;
                Standard_Boolean capping = Standard_False;
                if (clipping_tool->GetClippingPlane(plane_label, plane, plane_name, capping)) {
                    entry.planes.push_back(plane);
                }
            }
            cameras.emplace(name_of(entry.object->Name()), std::move(entry));
        }
    }
    for (size_t index = 0; index < pmi.view_count; ++index) {
        const occt_bridge_step_view_t& view = pmi.views[index];
        const auto [begin, end] = cameras.equal_range(view.name);
        const auto shown = presentations.views.find(view.name);
        uint32_t losses = OCCT_BRIDGE_PMI_VIEW_LOST;
        if (begin != end && shown != presentations.views.end()) {
            const auto [direction, up] = camera_axes(view);
            const bool same = std::any_of(begin, end, [&](const auto& entry) {
                return entry.second.object->ViewDirection().IsEqual(direction, 1e-7)
                    && entry.second.object->UpDirection().IsEqual(up, 1e-7);
            });
            const bool clipped = std::any_of(begin, end, [&](const auto& entry) {
                return same_planes(view, entry.second.planes);
            });
            losses = (same ? 0u : OCCT_BRIDGE_PMI_VIEW_LOST_CAMERA) | (clipped ? 0u : OCCT_BRIDGE_PMI_VIEW_LOST_CLIPPING);
        }
        if (pmi.out_view_losses != nullptr) {
            pmi.out_view_losses[index] = losses;
        }
        const auto lost = [&](const occt_bridge_step_presentation_t* presentation) -> uint8_t {
            return shown == presentations.views.end() || shown->second.count(presentation->name) == 0 ? 1 : 0;
        };
        for (size_t entry = 0; view.out_datums_lost != nullptr && entry < view.datum_count; ++entry) {
            view.out_datums_lost[entry] = lost(pmi.datums[view.datums[entry]].presentation);
        }
        for (size_t entry = 0; view.out_tolerances_lost != nullptr && entry < view.tolerance_count; ++entry) {
            view.out_tolerances_lost[entry] = lost(pmi.tolerances[view.tolerances[entry]].presentation);
        }
    }
    if (pmi.dataset != nullptr && pmi.dataset->out_losses != nullptr) {
        *pmi.dataset->out_losses = dataset_losses(reader.Reader().WS()->Model(), *pmi.dataset);
    }
    return true;
}

}  // namespace occt_bridge_internal
