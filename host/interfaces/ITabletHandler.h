#pragma once

#include <QTabletEvent>
#include <QPointF>
#include <QColor>
#include <cstdint>

// SOLID - Interface Segregation Principle (ISP):
// Segregates tablet hardware event processing (pressure, tilt dynamics) from generic mouse handling.
class ITabletHandler {
public:
    virtual ~ITabletHandler() = default;

    virtual bool handleTabletPress(QTabletEvent *event, uint64_t sessionHandle,
                                   const QPointF &docPoint, int baseDiameter,
                                   int hardness, int opacity, const QColor &color,
                                   bool isEraser) = 0;

    virtual bool handleTabletMove(QTabletEvent *event, uint64_t sessionHandle,
                                  const QPointF &docPoint, bool isPainting) = 0;

    virtual bool handleTabletRelease(QTabletEvent *event, uint64_t sessionHandle,
                                     bool isPainting) = 0;
};
