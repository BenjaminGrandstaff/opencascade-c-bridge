/*
 * Semantic AP242 GD&T for STEP assembly export: validation, XCAF labels on
 * part faces, and verification by reading the written file back.
 */

#pragma once

#include "occt_bridge.h"

#include <TDF_Label.hxx>
#include <TDocStd_Document.hxx>
#include <TopoDS_Shape.hxx>

#include <vector>

namespace occt_bridge_internal {

// The first problem with pmi for components placing shapes, or null.
const char* pmi_error(const occt_bridge_step_pmi_t& pmi, const std::vector<const TopoDS_Shape*>& shapes);

// Adds datums, tolerances and size dimensions on the faces of each
// component's part label.
void add_pmi(
    const Handle(TDocStd_Document)& document,
    const occt_bridge_step_pmi_t& pmi,
    const std::vector<TDF_Label>& component_parts,
    const std::vector<const TopoDS_Shape*>& shapes);

// Reads path back with OCCT's AP242 reader and fills pmi's loss arrays;
// false when the file cannot be read.
bool verify_pmi(const char* path, occt_bridge_step_pmi_t& pmi, const std::vector<const TopoDS_Shape*>& shapes);

}  // namespace occt_bridge_internal
