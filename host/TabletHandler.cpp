#include "TabletHandler.h"
#include <QString>
#include <QByteArray>
#include <algorithm>
#include <cstddef>

extern "C" int32_t compositor_session_command(uint64_t handle, const uint8_t *json, size_t count);
#include <algorithm>

bool PressureModulatedTabletHandler::handleTabletPress(QTabletEvent *event, uint64_t sessionHandle,
                                                      const QPointF &docPoint, int baseDiameter,
                                                      int hardness, int opacity, const QColor &color,
                                                      bool isEraser) {
    if (sessionHandle == 0 || !event) return false;
    const int erasing = isEraser ? 1 : 0;
    const qreal pressure = event->pressure() > 0.0 ? event->pressure() : 1.0;
    const int dynamicDiameter = std::max(1, qRound(baseDiameter * (0.2 + 0.8 * pressure)));

    const QString json = QString(
        R"({"version":1,"action":"brushBegin","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"red":%6,"green":%7,"blue":%8,"erasing":%9,"mask":0}})")
        .arg(docPoint.x(), 0, 'f', 4).arg(docPoint.y(), 0, 'f', 4)
        .arg(dynamicDiameter).arg(hardness / 100.0, 0, 'f', 3).arg(opacity / 100.0, 0, 'f', 3)
        .arg(color.redF(), 0, 'f', 4).arg(color.greenF(), 0, 'f', 4).arg(color.blueF(), 0, 'f', 4).arg(erasing);

    const QByteArray bytes = json.toUtf8();
    return compositor_session_command(sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0;
}

bool PressureModulatedTabletHandler::handleTabletMove(QTabletEvent *event, uint64_t sessionHandle,
                                                     const QPointF &docPoint, bool isPainting) {
    if (sessionHandle == 0 || !event || !isPainting) return false;
    const QString json = QString(R"({"version":1,"action":"brushMove","x":%1,"y":%2})")
        .arg(docPoint.x(), 0, 'f', 4).arg(docPoint.y(), 0, 'f', 4);
    const QByteArray bytes = json.toUtf8();
    return compositor_session_command(sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0;
}

bool PressureModulatedTabletHandler::handleTabletRelease(QTabletEvent *event, uint64_t sessionHandle,
                                                        bool isPainting) {
    if (sessionHandle == 0 || !event || !isPainting) return false;
    const QByteArray json = QString(R"({"version":1,"action":"brushEnd"})").toUtf8();
    return compositor_session_command(sessionHandle, reinterpret_cast<const uint8_t *>(json.constData()), json.size()) == 0;
}
