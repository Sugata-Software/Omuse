#include "EditorDialogs.h"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QLabel>
#include <QPushButton>
#include <QTimer>
#include <QVBoxLayout>
#include <memory>

FilterDialog::FilterDialog(const QString &kind, Submit submit, QWidget *parent) : QDialog(parent) {
    setObjectName("filterDialog"); setWindowTitle(kind);
    auto *layout = new QVBoxLayout(this);
    auto *form = new QFormLayout;
    layout->addLayout(form);
    auto parameters = std::make_shared<QJsonObject>();
    auto *debounce = new QTimer(this);
    debounce->setSingleShot(true); debounce->setInterval(100);
    auto addNumber = [=](const QString &label, const QString &key, double min, double max, double value, int decimals) {
        auto *field = new QDoubleSpinBox(this);
        field->setObjectName(key); field->setAccessibleName(label);
        field->setRange(min, max); field->setDecimals(decimals); field->setValue(value);
        field->setSingleStep(decimals ? 0.1 : 1); field->setKeyboardTracking(false);
        form->addRow(label, field); parameters->insert(key, value);
        connect(field, &QDoubleSpinBox::valueChanged, this, [=](double v) { parameters->insert(key, v); debounce->start(); });
    };
    auto addFlag = [=](const QString &label, const QString &key) {
        auto *field = new QCheckBox(label, this); field->setObjectName(key); layout->addWidget(field);
        parameters->insert(key, 0);
        connect(field, &QCheckBox::toggled, this, [=](bool v) { parameters->insert(key, v ? 1 : 0); debounce->start(); });
    };
    if (kind == "Gaussian Blur") addNumber(tr("&Radius (px)"), "radius", 0.1, 250, 1, 1);
    if (kind == "Motion Blur") {
        addNumber(tr("&Angle (°)"), "angle", -90, 90, 0, 0);
        addNumber(tr("&Distance (px)"), "distance", 1, 2000, 10, 0);
    }
    if (kind == "Add Noise") {
        addNumber(tr("&Amount (%)"), "amount", 0.1, 400, 10, 1);
        addFlag(tr("Gaussian distribution"), "gaussian"); addFlag(tr("Monochromatic"), "monochromatic");
    }
    if (kind == "Lens Correction") addNumber(tr("Remove &distortion"), "distortion", -100, 100, 0, 0);
    if (kind == "Grain") {
        addNumber(tr("&Amount"), "amount", 0, 100, 20, 0);
        addNumber(tr("&Size (px)"), "size", 0.5, 20, 2, 1);
        addNumber(tr("&Roughness"), "roughness", 0, 100, 50, 0);
    }
    if (kind == "Exposure") {
        addNumber(tr("&Exposure"), "exposure", -20, 20, 0, 2);
        addNumber(tr("&Offset"), "offset", -0.5, 0.5, 0, 4);
        addNumber(tr("&Gamma"), "gamma", 0.01, 9.99, 1, 2);
    }
    auto *preview = new QCheckBox(tr("Preview"), this);
    preview->setObjectName("preview"); preview->setChecked(true); layout->addWidget(preview);
    auto *error = new QLabel(this); error->setWordWrap(true); layout->addWidget(error);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    layout->addWidget(buttons);
    auto update = [=] {
        const bool ok = submit({{"action", "filterPreview"}, {"parameters", *parameters}});
        error->setText(ok ? QString() : tr("Preview failed. Adjust the settings or cancel."));
        buttons->button(QDialogButtonBox::Ok)->setEnabled(ok);
        return ok;
    };
    connect(debounce, &QTimer::timeout, this, [=] { update(); });
    connect(preview, &QCheckBox::toggled, this, [=](bool enabled) {
        submit({{"action", "filterSetPreview"}, {"enabled", enabled}});
    });
    connect(buttons, &QDialogButtonBox::accepted, this, [=] {
        debounce->stop();
        if (update() && submit({{"action", "filterCommit"}})) accept();
        else error->setText(tr("Could not apply the filter. Adjust the settings or cancel."));
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    // Escape, Cancel, and the window close button share the same rollback path.
    connect(this, &QDialog::rejected, this, [=] { debounce->stop(); submit({{"action", "filterCancel"}}); });
    // Defer work until the dialog is visible and can report a preview failure.
    debounce->start(0);
}
