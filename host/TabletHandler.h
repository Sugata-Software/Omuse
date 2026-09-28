#pragma once

#include "interfaces/ITabletHandler.h"

// SOLID - Single Responsibility Principle (SRP):
// Encapsulates tablet stylus pressure and tilt math, commanding the Swift core via C ABI.
class PressureModulatedTabletHandler : public ITabletHandler {
public:
    PressureModulatedTabletHandler() = default;

    bool handleTabletPress(QTabletEvent *event, uint64_t sessionHandle,
                           const QPointF &docPoint, int baseDiameter,
                           int hardness, int opacity, const QColor &color,
                           bool isEraser) override;

    bool handleTabletMove(QTabletEvent *event, uint64_t sessionHandle,
                          const QPointF &docPoint, bool isPainting) override;

    virtual bool handleTabletRelease(QTabletEvent *event, uint64_t sessionHandle,
                                     bool isPainting) override;
};
