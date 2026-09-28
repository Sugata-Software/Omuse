#pragma once

#include <QImage>
#include <QString>
#include <memory>

// SOLID - Interface Segregation Principle (ISP):
// A focused, role-segregated interface for image exporters.
// Decouples canvas consumers and UI from specific graphics codec backends.
class IImageExporter {
public:
    virtual ~IImageExporter() = default;
    virtual const char *format() const = 0;
    virtual bool exportImage(const QImage &image, const QString &filePath, int quality = 85) = 0;
};
