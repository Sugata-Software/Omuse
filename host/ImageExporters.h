#pragma once

#include "interfaces/IImageExporter.h"
#include <QMap>
#include <QString>
#include <memory>

// SOLID - Single Responsibility & Open/Closed Principles:
// Each exporter handles precisely one format's serialization nuances.

class PngImageExporter : public IImageExporter {
public:
    const char *format() const override { return "PNG"; }
    bool exportImage(const QImage &image, const QString &filePath, int quality = 85) override;
};

class JpegImageExporter : public IImageExporter {
public:
    const char *format() const override { return "JPEG"; }
    bool exportImage(const QImage &image, const QString &filePath, int quality = 85) override;
};

class TiffImageExporter : public IImageExporter {
public:
    const char *format() const override { return "TIFF"; }
    bool exportImage(const QImage &image, const QString &filePath, int quality = 85) override;
};

class WebPImageExporter : public IImageExporter {
public:
    const char *format() const override { return "WEBP"; }
    bool exportImage(const QImage &image, const QString &filePath, int quality = 85) override;
};

// SOLID - Dependency Inversion Principle (DIP):
// Registry/Factory providing runtime decoupling between consumers and concrete codecs.
class ImageExporterRegistry {
public:
    static ImageExporterRegistry &instance();

    void registerExporter(std::shared_ptr<IImageExporter> exporter);
    std::shared_ptr<IImageExporter> exporterForFormat(const QString &format) const;
    std::shared_ptr<IImageExporter> exporterForPath(const QString &filePath) const;

private:
    ImageExporterRegistry();
    QMap<QString, std::shared_ptr<IImageExporter>> m_exporters;
};
