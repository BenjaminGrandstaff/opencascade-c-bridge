/*
 * Semantic AP242 GD&T on STEP assembly parts: datums, geometric tolerances
 * and size dimensions attached to part faces through OCCT's XCAF dimension
 * and tolerance tool, and verified by reading the written file back.
 */

#include "step_pmi.hpp"

#include "bridge_internal.hpp"

#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Interface_InterfaceModel.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPControl_Reader.hxx>
#include <StepAP242_GeometricItemSpecificUsage.hxx>
#include <StepDimTol_Datum.hxx>
#include <StepRepr_ShapeAspect.hxx>
#include <StepRepr_ShapeAspectRelationship.hxx>
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

TDF_Label add_datum(
    const Handle(XCAFDoc_DimTolTool)& tool,
    const TDF_LabelSequence& faces,
    const char* label,
    int position,
    const std::vector<int>& modifiers) {
    const TDF_Label datum = tool->AddDatum();
    Handle(XCAFDimTolObjects_DatumObject) object = new XCAFDimTolObjects_DatumObject;
    object->SetName(text(label));
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
    if (read.type_of_value != zone) {
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
    const bool projected = tolerance.projected_mm > 0.0;
    const bool read_projected = read.zone == XCAFDimTolObjects_GeomToleranceZoneModif_Projected;
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
                pmi.datums[entry.datum].label,
                static_cast<int>(entry.compartment) + 1,
                datum_modifiers(entry));
            tool->SetDatumToGeomTol(datum, label);
        }
    }
    for (size_t index = 0; index < pmi.datum_count; ++index) {
        if (!referenced[index]) {
            add_datum(tool, datum_faces[index], pmi.datums[index].label, 0, {});
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

bool verify_pmi(const char* path, occt_bridge_step_pmi_t& pmi, const std::vector<const TopoDS_Shape*>& shapes) {
    STEPCAFControl_Reader reader;
    reader.SetGDTMode(Standard_True);
    reader.SetNameMode(Standard_True);
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
            pmi.out_datum_losses[index] = found ? 0u : OCCT_BRIDGE_PMI_LOST_ITEM;
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
    return true;
}

}  // namespace occt_bridge_internal
