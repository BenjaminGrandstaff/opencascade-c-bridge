/* Composition of explicitly chained operation histories, without changing geometry. */
#include "bridge_internal.hpp"

#include <TopExp.hxx>
#include <TopTools_IndexedMapOfShape.hxx>

using namespace occt_bridge_internal;

namespace {

std::vector<occt_bridge_history_entry> materialize(
    const occt_bridge_operation_history& history) {
    if (history.located_source.IsNull()) {
        return history.entries;
    }
    TopTools_IndexedMapOfShape sources;
    TopExp::MapShapes(history.located_source, sources);
    std::vector<occt_bridge_history_entry> entries;
    entries.reserve(static_cast<size_t>(sources.Extent()));
    for (int index = 1; index <= sources.Extent(); ++index) {
        const TopoDS_Shape& source = sources(index);
        entries.push_back({source, {}, {source.Moved(history.location)}, false});
    }
    return entries;
}

/*
 * Per-source target sets deduplicate by OCCT identity in expected O(1) time.
 * Generated ancestry wins when two branches reach the same output shape;
 * generated and modified relations remain disjoint, as in BRepTools_History.
 */
struct Targets {
    TopTools_IndexedMapOfShape generated;
    TopTools_IndexedMapOfShape modified;

    occt_bridge_history_entry entry(
        const TopoDS_Shape& source,
        const TopTools_IndexedMapOfShape& output) const {
        occt_bridge_history_entry result;
        result.source = source;
        for (int index = 1; index <= generated.Extent(); ++index) {
            result.generated.push_back(generated(index));
        }
        for (int index = 1; index <= modified.Extent(); ++index) {
            if (!generated.Contains(modified(index))) {
                result.modified.push_back(modified(index));
            }
        }
        result.deleted = !output.Contains(source) && result.modified.empty();
        return result;
    }
};

class Composer {
public:
    Composer(const TopoDS_Shape& output, const std::vector<occt_bridge_history_entry>& next)
        : next_(next) {
        TopExp::MapShapes(output, output_);
        for (const auto& entry : next_) {
            sources_.Add(entry.source);
        }
    }

    bool has_source(const TopoDS_Shape& shape) const {
        return sources_.Contains(shape);
    }

    occt_bridge_history_entry compose(const occt_bridge_history_entry& previous) const {
        Targets targets;
        for (const auto& shape : previous.generated) {
            map(shape, true, targets);
        }
        for (const auto& shape : previous.modified) {
            map(shape, false, targets);
        }
        // An unchanged source can still generate topology in the first step.
        // Its own downstream modifications must also be followed.
        if (!previous.deleted && previous.modified.empty()) {
            map(previous.source, false, targets, true);
        }
        return targets.entry(previous.source, output_);
    }

    occt_bridge_history_entry merge(
        const occt_bridge_history_entry& first,
        const occt_bridge_history_entry& second) const {
        Targets targets;
        for (const auto* entry : {&first, &second}) {
            for (const auto& shape : entry->generated) {
                targets.generated.Add(shape);
            }
            for (const auto& shape : entry->modified) {
                targets.modified.Add(shape);
            }
        }
        return targets.entry(first.source, output_);
    }

private:
    void map(
        const TopoDS_Shape& source,
        bool generated,
        Targets& targets,
        bool unchanged = false) const {
        const int index = sources_.FindIndex(source);
        if (index != 0) {
            const auto& next = next_[static_cast<size_t>(index - 1)];
            add(next.generated, targets.generated);
            add(next.modified, generated ? targets.generated : targets.modified);
        }
        if (output_.Contains(source) && !unchanged) {
            (generated ? targets.generated : targets.modified).Add(source);
        }
    }

    void add(const std::vector<TopoDS_Shape>& shapes, TopTools_IndexedMapOfShape& targets) const {
        for (const auto& shape : shapes) {
            if (output_.Contains(shape)) {
                targets.Add(shape);
            }
        }
    }

    const std::vector<occt_bridge_history_entry>& next_;
    TopTools_IndexedMapOfShape sources_;
    TopTools_IndexedMapOfShape output_;
};

/*
 * Expected O(topology + history records + expanded target relations) time and
 * memory, using shape-identity indexes; no repeated topology or graph scans.
 * A target with many ancestors costs one relation per ancestor, necessarily.
 * Rigid histories are expanded only for this explicitly requested composition;
 * ordinary placed copies retain their O(1) history representation.
 */
std::vector<occt_bridge_history_entry> compose_entries(
    const Composer& composer,
    std::vector<occt_bridge_history_entry> next,
    const std::vector<occt_bridge_history_entry>& previous) {
    TopTools_IndexedMapOfShape sources;
    for (const auto& entry : next) {
        sources.Add(entry.source);
    }
    for (const auto& entry : previous) {
        auto composed = composer.compose(entry);
        const int index = sources.FindIndex(entry.source);
        if (index != 0) {
            auto& existing = next[static_cast<size_t>(index - 1)];
            existing = composer.merge(existing, composed);
        } else {
            sources.Add(entry.source);
            next.push_back(std::move(composed));
        }
    }
    return next;
}

} // namespace

occt_bridge_status_t occt_bridge_shape_compose_history(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t intermediate,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* intermediate_shape = find_shape(session, intermediate);
        if (result_shape == nullptr || intermediate_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "composition shape was not found");
        }
        const auto next_history = session->histories.find(result);
        const auto previous_history = session->histories.find(intermediate);
        if (result == intermediate || next_history == session->histories.end()
            || previous_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "composition requires two operation histories");
        }
        const auto next = materialize(next_history->second);
        const Composer composer(*result_shape, next);
        if (!composer.has_source(*intermediate_shape)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intermediate is not an input to the result operation");
        }
        return store_shape_with_entries(
            session, *result_shape, out_shape,
            compose_entries(composer, next, materialize(previous_history->second)));
    });
}
