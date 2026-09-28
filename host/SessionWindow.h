#pragma once

// SessionWindow — a moc'd Qt window that drives the Swift editor core through the
// compositor_session_* C ABI, built by SwiftPM (with a committed moc output) and
// linked into the Swift `@main` composition root.
//
// This is distinct from MainWindow (the C++-main, C-kernels Qt shell): MainWindow
// is the "Rewrite Linux UI" workspace shell exercising the portable C kernels
// directly (no Swift), built via CMake for the C++-only build path. SessionWindow
// proves the architecture the Flatpak build ships — a moc'd Q_OBJECT Qt window
// created from compositor_host_run (the Swift-@main Qt entry) that calls back into
// the Swift core and paints the composited RGBA via QImage. As the Qt UI tier
// grows, MainWindow's shell will be rewired to drive compositor_session_* through
// this same link.
//
// Private C++ header in host/ (NOT host/include/, the Swift-importable C module),
// found via same-directory include.

#include <QMainWindow>
#include <functional>
#include <QImage>
#include <QString>
#include <QStringList>
#include <QPointF>
#include <QJsonObject>
#include <QJsonArray>
#include "interfaces/IPlatformServices.h"
#include "ParityMetrics.h"
#include "ParityPalette.h"

class QLockFile;
class QScrollArea;
class QTreeView;
class QStandardItemModel;
class QStandardItem;
class QSlider;
class QComboBox;
class QPushButton;
class QCheckBox;
class QMenu;
class QColor;
class QAction;
class QStackedWidget;
class QTabBar;
class QLabel;
class QToolBar;
class QSpinBox;
class QPainter;
class QKeyEvent;
#include <vector>
#include <memory>
#include <QMap>

class ITabletHandler;
class SessionCanvasWidget;

class SessionWindow : public QMainWindow {
    Q_OBJECT

public:
    // Platform services are injected (DIP); omitted members fall back to the Qt implementations.
    explicit SessionWindow(QWidget *parent = nullptr, PlatformServices services = {});
    ~SessionWindow() override;

    // Document management
    void initDemoDocument();
    void createNewDocument(int width, int height);
    bool hasDocument() const { return m_hasDocument; }
    // Exposes the Swift session handle so a debug-only caller (host_run.cpp's COMPOSITOR_GRAB_SWIFTUI_TREE path)
    // can render a SwiftUI-compat panel standalone, against this same session, without a second one.
    uint64_t sessionHandle() const { return m_sessionHandle; }

    // File operations (IO milestone)
    bool exportPNG(const QString &path);
    bool exportJPEG(const QString &path, int quality = 85);
    bool exportTIFF(const QString &path);
    bool exportWebP(const QString &path, int quality = 85);
    bool importImage(const QString &path, bool replacing = true);
    bool saveProject(const QString &path);
    bool loadProject(const QString &path);

    // Command Palette (Ctrl+Shift+P / F1)
    void showCommandPalette();
    void applyShortcuts();
    void syncSharedControls();
    void presentSharedColorPicker();
    void showTextEditor();
    void panCanvas(QPointF delta);
    void canvasKeyPressEvent(QKeyEvent *event) { keyPressEvent(event); }

    // Layer Multi-Selection (R22)
    void deleteSelectedLayers();

    // Crash-Recovery Autosave (R61)
    bool performAutosave();
    bool hasAutosaveRecovery() const;
    bool recoverAutosave();
    void clearAutosave();
    QString autosaveDirectory() const;
    void offerRecovery();
    bool hasUnsavedChanges() const;
    bool confirmDocumentReplacement();
    bool saveCurrentProject(bool saveAs = false);
    void updateDocumentTitle();

    // Snapshot of the Swift editor core's state JSON (test hook: reads the same
    // bytes the dock renders, through the real C ABI).
    QJsonObject sessionState() const;

    // Paint a stroke from the current palette (diameter/hardness/opacity/color)
    // through brushBegin/brushMove/brushEnd. Public so smokes can drive the
    // same code path the mouse handlers use.
    void paintStroke(double x1, double y1, double x2, double y2);
    // Same commands the CloneStamp/SpotHealing branches of mousePressEvent send; factored out so smoke tests exercise
    // the exact wire protocol without simulating screen-space mouse events.
    void setCloneSource(double x, double y);
    void cloneStroke(double x1, double y1, double x2, double y2);
    void healStroke(double x1, double y1, double x2, double y2);

    enum class Tool {
        Move = 0,    // V (default)
        Marquee,     // M (Rectangle / Ellipse mode)
        Lasso,       // L (Freehand / Polygonal mode)
        Magic,       // W (Wand / Object mode)
        Crop,        // C
        Brush,       // B (Eraser is a mode: E)
        SpotHealing, // J
        CloneStamp,  // S
        Smear,       // R (Liquify / Blur / Smudge mode)
        Gradient,    // G
        Shape,       // U (Rectangle / Ellipse / Line mode)
        Type,        // T
        Eyedropper,  // I
        Hand,        // H (Space temporary hand)
        Zoom,        // Z
        Idle,
        // Compatibility aliases
        Eraser = Brush,
        RectSelect = Marquee,
        EllipseSelect = Marquee,
        MagicWand = Magic,
        Blur = Smear
    };

    enum class MarqueeMode { Rectangle, Ellipse };
    enum class LassoMode { Freehand, Polygonal };
    enum class MagicMode { Wand, Object };
    enum class BrushToolMode { Paint, Erase };
    enum class SmearMode { Liquify, Blur, Smudge };
    enum class SpotHealingMode { ContentAware, CreateTexture, ProximityMatch };
    enum class ShapeMode { Rectangle, Ellipse, Line };

    void setTool(Tool tool);
    Tool currentTool() const { return m_tool; }

    void setMarqueeMode(MarqueeMode mode);
    MarqueeMode marqueeMode() const { return m_marqueeMode; }
    void cycleMarqueeMode();

    void setLassoMode(LassoMode mode);
    LassoMode lassoMode() const { return m_lassoMode; }
    void cycleLassoMode();

    void setMagicMode(MagicMode mode);
    MagicMode magicMode() const { return m_magicMode; }
    void toggleMagicMode();

    void setBrushToolMode(BrushToolMode mode);
    BrushToolMode brushToolMode() const { return m_brushToolMode; }

    void setSmearMode(SmearMode mode);
    SmearMode smearMode() const { return m_smearMode; }

    void setSpotHealingMode(SpotHealingMode mode);
    SpotHealingMode spotHealingMode() const { return m_spotHealingMode; }

    void setShapeMode(ShapeMode mode);
    ShapeMode shapeMode() const { return m_shapeMode; }
    void cycleShapeMode();

    // Pending Crop controls
    void applyCrop();
    void cancelCrop();
    bool hasPendingCrop() const { return m_hasPendingCrop; }
    QRectF pendingCropRect() const { return m_pendingCropRect; }

    // Space-hand controls
    bool isSpaceHandActive() const { return m_spaceHandActive; }

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void tabletEvent(QTabletEvent *event) override;
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dropEvent(QDropEvent *event) override;
    void closeEvent(QCloseEvent *event) override;
    bool eventFilter(QObject *watched, QEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;

    friend class SessionCanvasWidget;
    void canvasPaintEvent(QPaintEvent *event, QWidget *canvas);
    void canvasMousePressEvent(QMouseEvent *event, QWidget *canvas);
    void canvasMouseMoveEvent(QMouseEvent *event, QWidget *canvas);
    void canvasMouseReleaseEvent(QMouseEvent *event, QWidget *canvas);
    void canvasTabletEvent(QTabletEvent *event, QWidget *canvas);
    void canvasDragEnterEvent(QDragEnterEvent *event, QWidget *canvas);
    void canvasDropEvent(QDropEvent *event, QWidget *canvas);
    QRectF canvasTargetRect() const;
    QPointF documentToCanvasPoint(const QPointF &docPoint) const;

private:
    void createMenus();
    void refreshMenuTitles(const QJsonObject &state);
    void swapPaletteColors();
    void resetPaletteColors();
    void setBackgroundColor(const QColor &color);
    void pickBackgroundColor();
    void showSizeDialog(bool imageSize);
    void showFilterDialog(const QString &kind);
    void showAdjustDialog(const QString &kind);
    bool sendCommand(const QJsonObject &command);
    QPointF documentPoint(const QPointF &windowPoint) const;
    void refreshImage();
    void refreshLayers();
    void selectLayerRow(int row);
    void setOpacityFromSlider(int value);
    void setBrushDiameter(int value);
    void setBrushHardness(int value);
    void setBrushOpacity(int value);
    void setBrushColor(const QColor &color);
    void setBlendModeFromCombo(int index);
    void pickBrushColor();
    QStringList blendModes() const;
    bool setLayerFlag(const char *action, bool on);
    void selectRegion(bool rectangle);
    void applyDarkTheme();
    void setupHeaderBar();
    void setupOptionsBar();
    void updateOptionsBar();
    void updateToolRail();
    void syncToolFromSession();
    void updateStatusTelemetry();
    void fitCanvas();
    void actualPixels();
    void zoomBy(double factor);

    QWidget *m_canvasWidget = nullptr;
    QImage m_image;
    QPointF m_lastPoint;
    QJsonArray m_shortcuts;
    bool m_mappingShortcut = false;
    bool m_presentingColor = false;
    bool m_presentingText = false;
    uint64_t m_sessionHandle = 0;
    bool m_painting = false;
    Tool m_tool = Tool::Move;
    MarqueeMode m_marqueeMode = MarqueeMode::Rectangle;
    LassoMode m_lassoMode = LassoMode::Freehand;
    MagicMode m_magicMode = MagicMode::Wand;
    BrushToolMode m_brushToolMode = BrushToolMode::Paint;
    SmearMode m_smearMode = SmearMode::Liquify;
    SpotHealingMode m_spotHealingMode = SpotHealingMode::ContentAware;
    ShapeMode m_shapeMode = ShapeMode::Rectangle;
    QRectF m_pendingCropRect;
    bool m_hasPendingCrop = false;
    QString m_cropRatio = "Free";
    Tool m_preSpaceTool = Tool::Move;
    bool m_spaceHandActive = false;
    QPointF m_panOffset{0, 0};
    bool m_hasDocument = false;

    QPointF m_dragStart;
    QPointF m_currentPoint;
    std::vector<QPointF> m_lassoPoints;
    // The source's own point/offset live server-side (EditorSession.cloneSource/cloneOffset); this only tracks
    // whether one has been set, so a stroke before an Option-click is refused with a message instead of a silent no-op.
    bool m_hasCloneSource = false;
    QString m_brushMode = "Paint";
    QTreeView *m_layersView = nullptr;
    QStandardItemModel *m_layerModel = nullptr;
    QSlider *m_opacity = nullptr;
    QLabel *m_opacityLabel = nullptr;
    QLabel *m_layerCountLabel = nullptr;
    QComboBox *m_blend = nullptr;
    QPushButton *m_brushColorButton = nullptr;
    QSlider *m_brushDiameterSlider = nullptr;
    QSlider *m_brushHardnessSlider = nullptr;
    QSlider *m_brushOpacitySlider = nullptr;
    QCheckBox *m_visibleCheck = nullptr;
    QCheckBox *m_maskCheck = nullptr;
    QColor m_brushColor = ParityPalette::defaultForeground();
    QColor m_backgroundColor = ParityPalette::defaultBackground();
    QPushButton *m_bgColorButton = nullptr;
    int m_brushDiameter = 16;
    int m_brushHardness = 100;
    int m_brushOpacity = 100;
    bool m_syncingLayers = false;
    QMap<Tool, QAction *> m_toolActions;
    std::unique_ptr<ITabletHandler> m_tabletHandler;
    QTimer *m_autosaveTimer = nullptr;
    QString m_projectPath;
    QString m_recoveryID;
    QString m_recoveredDirectory;
    bool m_recoveredUnsaved = false;
    std::unique_ptr<QLockFile> m_recoveryLock;
    std::unique_ptr<QLockFile> m_recoveredLock;
    QToolBar *m_headerToolBar = nullptr;
    QToolBar *m_optionsToolBar = nullptr;
    QToolBar *m_toolsBar = nullptr;
    QDockWidget *m_layersDock = nullptr;
    QDockWidget *m_adjustmentsDock = nullptr;
    QStackedWidget *m_optionsStack = nullptr;
    QWidget *m_swiftUIOptionsContainer = nullptr;
    QScrollArea *m_toolOptionsScroll = nullptr;
    QWidget *m_swiftUICurrentToolHeader = nullptr;
    QWidget *m_swiftUIToolRailContainer = nullptr;
    QWidget *m_swiftUICurrentToolRail = nullptr;
    QWidget *m_swiftUIStatusBarContainer = nullptr;
    QWidget *m_swiftUICurrentStatusBar = nullptr;
    QTabBar *m_documentTabBar = nullptr;
    QLabel *m_statusZoomLabel = nullptr;
    QLabel *m_statusDimsLabel = nullptr;
    QLabel *m_statusProfileLabel = nullptr;
    QLabel *m_statusHintsLabel = nullptr;
    double m_zoomLevel = 0.0;
    PlatformServices m_platform;

    // Dynamic menu action pointers for macOS parity
    QAction *m_actUndo = nullptr;
    QAction *m_actRedo = nullptr;
    QAction *m_actCut = nullptr;
    QAction *m_actCopy = nullptr;
    QAction *m_actCopyMerged = nullptr;
    QAction *m_actPaste = nullptr;
    QAction *m_actFillFG = nullptr;
    QAction *m_actFillBG = nullptr;
    QAction *m_actClearSelection = nullptr;
    QAction *m_actContentAwareFill = nullptr;
    QAction *m_actSelectAll = nullptr;
    QAction *m_actDeselect = nullptr;
    QAction *m_actInverse = nullptr;
    QAction *m_actLayerPixels = nullptr;
    QAction *m_actSelectSubject = nullptr;
    QAction *m_actMaskBlackAreas = nullptr;
    QAction *m_actExpandSelection = nullptr;
    QAction *m_actContractSelection = nullptr;
    QAction *m_actFeatherSelection = nullptr;
    QAction *m_actInvert = nullptr;
    QAction *m_actTransform = nullptr;
    QAction *m_actDuplicate = nullptr;
    QAction *m_actClippingMask = nullptr;
    QAction *m_actGroupLayers = nullptr;
    QAction *m_actMoveOutOfFolder = nullptr;
    QAction *m_actNewBlankLayer = nullptr;
    QAction *m_actRenameLayer = nullptr;
    QAction *m_actShowHideLayer = nullptr;
    QAction *m_actMoveLayerUp = nullptr;
    QAction *m_actMoveLayerDown = nullptr;
    QAction *m_actMerge = nullptr;
    QAction *m_actFlipLayerH = nullptr;
    QAction *m_actFlipLayerV = nullptr;
    QAction *m_actDelete = nullptr;

    // One-shot canvas pixel request (Levels eyedroppers): receives the document point of the next click.
    std::function<void(const QPointF &)> m_pixelSampler;

    // Move / Transform tool state. Geometry is in document pixels; rotation in degrees.
public:
    struct LayerGeometry {
        QString id;
        double x = 0, y = 0, w = 0, h = 0, rotation = 0;
        bool valid = false, isGroup = false, visible = true;
    };
private:
    LayerGeometry m_activeGeometry;
    std::vector<LayerGeometry> m_layerGeometries;  // document order, bottom first
    QSpinBox *m_xSpin = nullptr;
    QSpinBox *m_ySpin = nullptr;
    QSpinBox *m_wSpin = nullptr;
    QSpinBox *m_hSpin = nullptr;
    QSpinBox *m_angleSpin = nullptr;
    QCheckBox *m_linkCheck = nullptr;
    QCheckBox *m_autoSelectCheck = nullptr;
    QCheckBox *m_showControlsCheck = nullptr;
    int m_transformHandle = -1;  // -1 none, 0-7 resize (TL,T,TR,R,BR,B,BL,L), 8 rotate, 9 move body
    LayerGeometry m_transformStart;
    LayerGeometry m_transformDraft;
    // Selection tools: combine mode (New/Add/Subtract) and lasso style.
    QString m_selectionMode = "New";
    bool m_polygonalLasso = false;
    bool m_polyActive = false;
    qint64 m_lastPolyClick = 0;
    void commitLassoSelection();
    void syncTransformFields();
    void applyTransformFields(int changedField);
    int hitTestTransformHandle(const QPointF &canvasPoint) const;
    void drawTransformControls(QPainter &painter) const;
    void previewGeometry(const LayerGeometry &geometry);
    LayerGeometry draggedGeometry(const QPointF &documentPoint, Qt::KeyboardModifiers modifiers) const;

    // Tool parameter state
    int m_magicTolerance = 32;
    bool m_magicContiguous = true;
    bool m_magicSampleAll = false;
    bool m_cloneAligned = true;
    bool m_cloneSampleAll = false;
    QString m_gradientType = "Linear";
    int m_gradientOpacity = 100;
    bool m_gradientReverse = false;
    int m_shapeStrokeWidth = 1;
    int m_shapeRadius = 0;
    int m_typeFontSize = 24;
    QString m_typeFontFamily;
    int m_typeAlignment = 0;
    int m_eyedropperSampleSize = 1;
    bool m_eyedropperSampleAll = false;
    int m_cropHandle = -1;
    QPointF m_panStart;

    int hitTestCropHandle(const QPointF &canvasPoint) const;
    void drawCropOverlay(QPainter &painter) const;
};
